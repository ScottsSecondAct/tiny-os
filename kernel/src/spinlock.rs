use core::sync::atomic::{AtomicU32, Ordering};

/// Ticket spinlock for SMP mutual exclusion.
///
/// Disables IRQs on the local core before acquiring the lock to prevent
/// deadlock (IRQ handler trying to acquire a lock held by the interrupted code).
/// IRQ state is restored when the lock is released.
pub struct SpinLock {
    ticket: AtomicU32,
    serving: AtomicU32,
}

// SAFETY: SpinLock is designed for cross-core mutual exclusion.
unsafe impl Sync for SpinLock {}
unsafe impl Send for SpinLock {}

impl SpinLock {
    pub const fn new() -> Self {
        Self {
            ticket: AtomicU32::new(0),
            serving: AtomicU32::new(0),
        }
    }

    /// Acquire the lock. Returns the saved DAIF state for restoration on unlock.
    pub fn lock(&self) -> u64 {
        let daif: u64;
        unsafe {
            core::arch::asm!("mrs {}, daif", out(reg) daif);
            core::arch::asm!("msr daifset, #2");
        }

        let my_ticket = self.ticket.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != my_ticket {
            core::hint::spin_loop();
        }

        daif
    }

    /// Release the lock and restore the saved DAIF state.
    pub fn unlock(&self, saved_daif: u64) {
        let next = self.serving.load(Ordering::Relaxed) + 1;
        self.serving.store(next, Ordering::Release);

        if saved_daif & (1 << 7) == 0 {
            unsafe { core::arch::asm!("msr daifclr, #2") };
        }
    }

    /// Release the lock without restoring DAIF.
    /// Used by task_trampoline for newly created tasks that enter for the first
    /// time (they never called lock, so there is no saved DAIF to restore).
    pub unsafe fn force_unlock(&self) {
        let next = self.serving.load(Ordering::Relaxed) + 1;
        self.serving.store(next, Ordering::Release);
    }
}
