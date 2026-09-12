use crate::os_cfg;
use crate::sched::{self, CriticalSection, WaitResult};
use core::cell::UnsafeCell;

const MAX_EVENT_WAITERS: usize = os_cfg::MAX_EVENT_WAITERS;
const NONE: u8 = 0xFF;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EventWaitMode {
    Any,
    All,
}

#[derive(Clone, Copy)]
struct Waiter {
    task_id: u8,
    mask: u32,
    mode: EventWaitMode,
}

struct EventInner {
    flags: u32,
    waiters: [Waiter; MAX_EVENT_WAITERS],
    waiter_count: u8,
}

pub struct EventFlags {
    inner: UnsafeCell<EventInner>,
}

// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for EventFlags {}

impl EventFlags {
    pub const fn new() -> Self {
        const EMPTY_WAITER: Waiter = Waiter {
            task_id: NONE,
            mask: 0,
            mode: EventWaitMode::Any,
        };
        Self {
            inner: UnsafeCell::new(EventInner {
                flags: 0,
                waiters: [EMPTY_WAITER; MAX_EVENT_WAITERS],
                waiter_count: 0,
            }),
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn inner(&self) -> &mut EventInner {
        // SAFETY: Caller holds CriticalSection.
        unsafe { &mut *self.inner.get() }
    }

    /// Set bits in the event group. Wakes any waiters whose conditions are met.
    pub fn set(&self, bits: u32) {
        let _cs = CriticalSection::enter();
        let e = self.inner();
        e.flags |= bits;
        self.wake_satisfied(e);
    }

    /// Clear bits in the event group.
    pub fn clear(&self, bits: u32) {
        let _cs = CriticalSection::enter();
        self.inner().flags &= !bits;
    }

    /// Get current flags value.
    pub fn get(&self) -> u32 {
        let _cs = CriticalSection::enter();
        self.inner().flags
    }

    /// Wait for event bits. Returns the flags that satisfied the wait.
    pub fn wait(&self, mask: u32, mode: EventWaitMode) -> Result<u32, &'static str> {
        self.wait_impl(mask, mode, 0)
    }

    pub fn wait_timeout(
        &self,
        mask: u32,
        mode: EventWaitMode,
        ticks: u32,
    ) -> Result<u32, &'static str> {
        self.wait_impl(mask, mode, ticks)
    }

    fn wait_impl(&self, mask: u32, mode: EventWaitMode, timeout: u32) -> Result<u32, &'static str> {
        let _cs = CriticalSection::enter();
        let e = self.inner();

        if self.condition_met(e.flags, mask, mode) {
            return Ok(e.flags & mask);
        }

        let me = sched::current_id();
        if e.waiter_count as usize >= MAX_EVENT_WAITERS {
            return Err("events: too many waiters");
        }

        let idx = e.waiter_count as usize;
        e.waiters[idx] = Waiter {
            task_id: me,
            mask,
            mode,
        };
        e.waiter_count += 1;

        sched::set_task_wait_result(me, WaitResult::Timeout);

        if timeout > 0 {
            sched::block_current_timeout(timeout);
        } else {
            sched::block_current();
        }

        // Remove our waiter entry.
        self.remove_waiter(me);

        if sched::get_wait_result() == WaitResult::Timeout {
            return Err("events: timeout");
        }

        let flags = self.inner().flags;
        Ok(flags & mask)
    }

    fn condition_met(&self, flags: u32, mask: u32, mode: EventWaitMode) -> bool {
        match mode {
            EventWaitMode::Any => (flags & mask) != 0,
            EventWaitMode::All => (flags & mask) == mask,
        }
    }

    fn wake_satisfied(&self, e: &mut EventInner) {
        let mut i = 0;
        while i < e.waiter_count as usize {
            let w = &e.waiters[i];
            if self.condition_met(e.flags, w.mask, w.mode) {
                let id = w.task_id;
                if sched::get_state(id) == sched::TaskState::Blocked {
                    sched::set_task_wait_result(id, WaitResult::Ok);
                    sched::wake_task(id);
                }
                // Remove this waiter.
                let last = e.waiter_count as usize - 1;
                e.waiters[i] = e.waiters[last];
                e.waiters[last].task_id = NONE;
                e.waiter_count -= 1;
            } else {
                i += 1;
            }
        }
    }

    fn remove_waiter(&self, task_id: u8) {
        let e = self.inner();
        for i in 0..e.waiter_count as usize {
            if e.waiters[i].task_id == task_id {
                let last = e.waiter_count as usize - 1;
                e.waiters[i] = e.waiters[last];
                e.waiters[last].task_id = NONE;
                e.waiter_count -= 1;
                return;
            }
        }
    }
}
