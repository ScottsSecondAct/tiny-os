use bsp::PlatformUart;
use core::cell::UnsafeCell;
use core::fmt;

use crate::spinlock::SpinLock;

struct GlobalWriter(UnsafeCell<Option<PlatformUart>>);

// SAFETY: Access is guarded by PRINT_LOCK spinlock, preventing concurrent
// access from multiple cores and interrupt handlers.
unsafe impl Sync for GlobalWriter {}

static WRITER: GlobalWriter = GlobalWriter(UnsafeCell::new(None));
static PRINT_LOCK: SpinLock = SpinLock::new();

pub fn init(uart: PlatformUart) {
    // SAFETY: Called once from kmain before any concurrent access.
    unsafe {
        *WRITER.0.get() = Some(uart);
    }
}

pub fn _print(args: fmt::Arguments) {
    use fmt::Write;

    let saved = PRINT_LOCK.lock();

    // SAFETY: Protected by PRINT_LOCK spinlock.
    if let Some(writer) = unsafe { (*WRITER.0.get()).as_mut() } {
        let _ = writer.write_fmt(args);
    }

    PRINT_LOCK.unlock(saved);
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! kprintln {
    ()                => { $crate::kprint!("\n") };
    ($($arg:tt)*)     => { $crate::kprint!("{}\n", core::format_args!($($arg)*)) };
}
