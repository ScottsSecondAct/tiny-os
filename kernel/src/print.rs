// Kernel print subsystem.
//
// Provides a process-global UART writer and the `kprint!` / `kprintln!`
// macros used throughout the kernel.
//
// Phase 1 safety note: We use `UnsafeCell<Option<…>>` with a manual
// `unsafe impl Sync` because in Phase 1 we are strictly single-core with
// all interrupts masked (DAIF set by boot.S). No concurrent access is
// possible, so the invariant is trivially maintained. This will be
// replaced with a proper spinlock-protected writer in Phase 7 (SMP).

use bsp::PlatformUart;
use core::cell::UnsafeCell;
use core::fmt;

struct GlobalWriter(UnsafeCell<Option<PlatformUart>>);

// SAFETY: Phase 1 is single-core with interrupts masked; no concurrent
// access to the UART writer can occur.
unsafe impl Sync for GlobalWriter {}

static WRITER: GlobalWriter = GlobalWriter(UnsafeCell::new(None));

/// Install the UART writer. Must be called exactly once, from `kmain`,
/// before any `kprint!` invocation.
pub fn init(uart: PlatformUart) {
    // SAFETY: Called once before any concurrent access; single-core Phase 1.
    unsafe {
        *WRITER.0.get() = Some(uart);
    }
}

/// Internal: write `fmt::Arguments` to the global UART writer.
/// Silently drops output if `init` has not been called yet.
pub fn _print(args: fmt::Arguments) {
    use fmt::Write;
    // SAFETY: Single-core, interrupts masked in Phase 1.
    if let Some(writer) = unsafe { (*WRITER.0.get()).as_mut() } {
        let _ = writer.write_fmt(args);
    }
}

/// Print without a newline.
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!($($arg)*))
    };
}

/// Print with a trailing newline.
#[macro_export]
macro_rules! kprintln {
    ()                => { $crate::kprint!("\n") };
    ($($arg:tt)*)     => { $crate::kprint!("{}\n", core::format_args!($($arg)*)) };
}
