use core::cell::UnsafeCell;
use crate::sched::{self, CriticalSection, WaitResult};
use super::WaitQueue;

struct SemInner {
    count: u32,
    max_count: u32,
    waiters: WaitQueue,
}

pub struct Semaphore {
    inner: UnsafeCell<SemInner>,
}

// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// Create a counting semaphore. `init` is the initial count, `max` the upper bound.
    /// For a binary semaphore, use `init=0, max=1` or `init=1, max=1`.
    pub const fn new(init: u32, max: u32) -> Self {
        Self {
            inner: UnsafeCell::new(SemInner {
                count: init,
                max_count: max,
                waiters: WaitQueue::new(),
            }),
        }
    }

    pub const fn binary(init: u32) -> Self {
        Self::new(init, 1)
    }

    fn inner(&self) -> &mut SemInner {
        // SAFETY: Caller holds CriticalSection.
        unsafe { &mut *self.inner.get() }
    }

    /// Decrement (wait/pend). Blocks if count is 0.
    pub fn wait(&self) -> Result<(), &'static str> {
        self.wait_impl(0)
    }

    pub fn wait_timeout(&self, ticks: u32) -> Result<(), &'static str> {
        self.wait_impl(ticks)
    }

    fn wait_impl(&self, timeout: u32) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let s = self.inner();

        if s.count > 0 {
            s.count -= 1;
            return Ok(());
        }

        let me = sched::current_id();
        s.waiters.add(me);
        sched::set_task_wait_result(me, WaitResult::Timeout);

        if timeout > 0 {
            sched::block_current_timeout(timeout);
        } else {
            sched::block_current();
        }

        if sched::get_wait_result() == WaitResult::Timeout {
            s.waiters.remove(me);
            return Err("semaphore: timeout");
        }

        Ok(())
    }

    pub fn try_wait(&self) -> bool {
        let _cs = CriticalSection::enter();
        let s = self.inner();
        if s.count > 0 {
            s.count -= 1;
            true
        } else {
            false
        }
    }

    /// Increment (signal/post). Wakes the highest-priority waiter if any.
    pub fn post(&self) -> Result<(), &'static str> {
        let _cs = CriticalSection::enter();
        let s = self.inner();

        s.waiters.cleanup_stale();

        if let Some(waiter) = s.waiters.pop_highest() {
            sched::set_task_wait_result(waiter, WaitResult::Ok);
            sched::wake_task(waiter);
            return Ok(());
        }

        if s.count >= s.max_count {
            return Err("semaphore: overflow");
        }
        s.count += 1;
        Ok(())
    }

    pub fn count(&self) -> u32 {
        let _cs = CriticalSection::enter();
        self.inner().count
    }
}
