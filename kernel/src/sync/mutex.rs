use core::cell::UnsafeCell;
use crate::sched::{self, CriticalSection, WaitResult};
use super::WaitQueue;

const NONE: u8 = 0xFF;
const MAX_NEST: u8 = 8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MutexProtocol {
    None,
    PriorityInheritance,
    PriorityCeiling(u8),
}

struct MutexInner {
    owner: u8,
    nest_count: u8,
    protocol: MutexProtocol,
    saved_prio: u8,
    waiters: WaitQueue,
}

pub struct Mutex {
    inner: UnsafeCell<MutexInner>,
}

// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for Mutex {}

impl Mutex {
    pub const fn new(protocol: MutexProtocol) -> Self {
        Self {
            inner: UnsafeCell::new(MutexInner {
                owner: NONE,
                nest_count: 0,
                protocol,
                saved_prio: 0,
                waiters: WaitQueue::new(),
            }),
        }
    }

    fn inner(&self) -> &mut MutexInner {
        // SAFETY: Caller holds CriticalSection.
        unsafe { &mut *self.inner.get() }
    }

    pub fn lock(&self) -> Result<(), &'static str> {
        self.lock_impl(0)
    }

    pub fn lock_timeout(&self, ticks: u32) -> Result<(), &'static str> {
        if ticks == 0 {
            return self.lock_impl(0);
        }
        self.lock_impl(ticks)
    }

    fn lock_impl(&self, timeout: u32) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let m = self.inner();
        let me = sched::current_id();

        if m.owner == NONE {
            m.owner = me;
            m.nest_count = 1;
            match m.protocol {
                MutexProtocol::PriorityCeiling(ceil) => {
                    m.saved_prio = sched::get_priority(me);
                    if ceil < m.saved_prio {
                        sched::set_priority(me, ceil);
                    }
                }
                _ => {
                    m.saved_prio = sched::get_priority(me);
                }
            }
            return Ok(());
        }

        if m.owner == me {
            if m.nest_count >= MAX_NEST {
                return Err("mutex: max nesting depth exceeded");
            }
            m.nest_count += 1;
            return Ok(());
        }

        // Mutex held by another task — block.
        if m.protocol == MutexProtocol::PriorityInheritance {
            let my_prio = sched::get_priority(me);
            let owner_prio = sched::get_priority(m.owner);
            if my_prio < owner_prio {
                sched::set_priority(m.owner, my_prio);
            }
        }

        m.waiters.add(me);
        sched::set_task_wait_result(me, WaitResult::Timeout);

        if timeout > 0 {
            sched::block_current_timeout(timeout);
        } else {
            sched::block_current();
        }

        // Woken up — check if we got the mutex or timed out.
        if sched::get_wait_result() == WaitResult::Timeout {
            m.waiters.remove(me);
            // Recalculate PIP for owner if needed.
            if m.protocol == MutexProtocol::PriorityInheritance && m.owner != NONE {
                self.recalc_pip(m);
            }
            return Err("mutex: timeout");
        }

        Ok(())
    }

    pub fn try_lock(&self) -> bool {
        let _cs = CriticalSection::enter();
        let m = self.inner();
        let me = sched::current_id();

        if m.owner == NONE {
            m.owner = me;
            m.nest_count = 1;
            match m.protocol {
                MutexProtocol::PriorityCeiling(ceil) => {
                    m.saved_prio = sched::get_priority(me);
                    if ceil < m.saved_prio {
                        sched::set_priority(me, ceil);
                    }
                }
                _ => {
                    m.saved_prio = sched::get_priority(me);
                }
            }
            true
        } else if m.owner == me {
            if m.nest_count < MAX_NEST {
                m.nest_count += 1;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn unlock(&self) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let m = self.inner();
        let me = sched::current_id();

        if m.owner != me {
            return Err("mutex: not owner");
        }

        m.nest_count -= 1;
        if m.nest_count > 0 {
            return Ok(());
        }

        // Restore owner's priority.
        match m.protocol {
            MutexProtocol::PriorityInheritance | MutexProtocol::PriorityCeiling(_) => {
                sched::set_priority(me, m.saved_prio);
            }
            MutexProtocol::None => {}
        }

        m.owner = NONE;
        m.waiters.cleanup_stale();

        // Wake highest-priority waiter and grant them the mutex.
        if let Some(waiter) = m.waiters.pop_highest() {
            m.owner = waiter;
            m.nest_count = 1;
            match m.protocol {
                MutexProtocol::PriorityCeiling(ceil) => {
                    m.saved_prio = sched::get_priority(waiter);
                    if ceil < m.saved_prio {
                        sched::set_priority(waiter, ceil);
                    }
                }
                MutexProtocol::PriorityInheritance => {
                    m.saved_prio = sched::get_base_priority(waiter);
                }
                MutexProtocol::None => {
                    m.saved_prio = sched::get_priority(waiter);
                }
            }
            sched::set_task_wait_result(waiter, WaitResult::Ok);
            sched::wake_task(waiter);
        }

        Ok(())
    }

    fn recalc_pip(&self, m: &mut MutexInner) {
        let owner = m.owner;
        let base = sched::get_base_priority(owner);
        let waiter_prio = m.waiters.highest_blocked_priority();
        let new_prio = match waiter_prio {
            Some(p) if p < base => p,
            _ => base,
        };
        sched::set_priority(owner, new_prio);
    }

    pub fn owner(&self) -> Option<u8> {
        let _cs = CriticalSection::enter();
        let m = self.inner();
        if m.owner == NONE { None } else { Some(m.owner) }
    }
}
