use core::cell::UnsafeCell;
use crate::sched::CriticalSection;

struct WatchdogState {
    enabled: bool,
    timeout_ticks: u32,
    counter: u32,
    kick_count: u64,
}

impl WatchdogState {
    const fn new() -> Self {
        Self {
            enabled: false,
            timeout_ticks: 0,
            counter: 0,
            kick_count: 0,
        }
    }
}

struct WdogCell(UnsafeCell<WatchdogState>);
// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for WdogCell {}

static WDOG: WdogCell = WdogCell(UnsafeCell::new(WatchdogState::new()));

fn state() -> &'static mut WatchdogState {
    // SAFETY: Caller holds CriticalSection or is in ISR with IRQs masked.
    unsafe { &mut *WDOG.0.get() }
}

pub fn init(timeout_ms: u32) {
    let _cs = CriticalSection::enter();
    let s = state();
    s.timeout_ticks = timeout_ms;
    s.counter = 0;
    s.kick_count = 0;
    s.enabled = true;
}

pub fn kick() {
    let _cs = CriticalSection::enter();
    let s = state();
    s.counter = 0;
    s.kick_count += 1;
}

/// Called from sched::tick() (ISR context, IRQs already masked).
pub fn tick() {
    let s = state();
    if !s.enabled {
        return;
    }
    s.counter += 1;
    if s.counter >= s.timeout_ticks {
        s.enabled = false;
        panic!("SOFTWARE WATCHDOG TIMEOUT: no kick in {}ms", s.timeout_ticks);
    }
}

pub fn is_enabled() -> bool {
    let _cs = CriticalSection::enter();
    state().enabled
}

pub fn counter() -> u32 {
    let _cs = CriticalSection::enter();
    state().counter
}

pub fn timeout() -> u32 {
    let _cs = CriticalSection::enter();
    state().timeout_ticks
}

pub fn kick_count() -> u64 {
    let _cs = CriticalSection::enter();
    state().kick_count
}
