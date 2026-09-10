// ARM Generic Timer driver for tiny_os.
//
// Uses the EL1 Virtual Timer (CNTV):
//   CNTV_CTL_EL0  — control register (enable, mask, status)
//   CNTV_TVAL_EL0 — down-counter; fires when it reaches 0
//   CNTV_CVAL_EL0 — compare value (absolute)
//   CNTVCT_EL0    — virtual counter (read-only)
//   CNTFRQ_EL0    — timer frequency in Hz (read-only)
//
// The virtual timer PPI is INTID 27.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::timer::Timer;

pub const TIMER_IRQ_ID: u32 = 27;

pub struct GenericTimer {
    reload_value: u32,
    freq: u64,
}

struct TimerCell(UnsafeCell<GenericTimer>);
// SAFETY: Single-core; written once during init, then accessed only from
// IRQ context which is serialized.
unsafe impl Sync for TimerCell {}

static TIMER: TimerCell = TimerCell(UnsafeCell::new(GenericTimer {
    reload_value: 0,
    freq: 0,
}));

static TIMER_FREQ: AtomicU64 = AtomicU64::new(0);

impl Timer for GenericTimer {
    fn init(&mut self, tick_rate_hz: u32) {
        let freq: u64;
        unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq) };
        self.freq = freq;
        TIMER_FREQ.store(freq, Ordering::Relaxed);
        self.reload_value = (freq / tick_rate_hz as u64) as u32;

        unsafe { core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) 0u64) };
        unsafe { core::arch::asm!("msr cntv_tval_el0, {}", in(reg) self.reload_value as u64) };
        // ENABLE=1, IMASK=0
        unsafe { core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) 1u64) };
    }

    fn acknowledge(&mut self) {
        // Advance CVAL by reload_value so the period is measured from the
        // previous deadline, not from now — absorbs handler latency.
        let cval: u64;
        unsafe { core::arch::asm!("mrs {}, cntv_cval_el0", out(reg) cval) };
        let next = cval.wrapping_add(self.reload_value as u64);
        unsafe { core::arch::asm!("msr cntv_cval_el0, {}", in(reg) next) };
    }

    fn read_counter(&self) -> u64 {
        let cnt: u64;
        unsafe { core::arch::asm!("mrs {}, cntvct_el0", out(reg) cnt) };
        cnt
    }

    fn ticks_per_second(&self) -> u64 {
        self.freq
    }
}

pub fn init(tick_rate_hz: u32) {
    // SAFETY: Called once during single-core init before interrupts are enabled.
    unsafe { (*TIMER.0.get()).init(tick_rate_hz) }
}

pub fn handle_tick() {
    // SAFETY: Called from IRQ context; single-core, no reentrancy.
    unsafe { (*TIMER.0.get()).acknowledge() }
    super::exceptions::increment_tick();
}

pub fn read_counter() -> u64 {
    // SAFETY: Read-only register access.
    unsafe { (*TIMER.0.get()).read_counter() }
}

pub fn frequency() -> u64 {
    TIMER_FREQ.load(Ordering::Relaxed)
}
