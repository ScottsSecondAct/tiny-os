use bsp::PlatformUart;
use core::cell::UnsafeCell;
use core::fmt;

struct GlobalWriter(UnsafeCell<Option<PlatformUart>>);

// SAFETY: Access is guarded by DAIF IRQ masking in `_print`, preventing
// concurrent access from interrupt handlers on this single core.
unsafe impl Sync for GlobalWriter {}

static WRITER: GlobalWriter = GlobalWriter(UnsafeCell::new(None));

pub fn init(uart: PlatformUart) {
    // SAFETY: Called once from kmain before any concurrent access.
    unsafe {
        *WRITER.0.get() = Some(uart);
    }
}

pub fn _print(args: fmt::Arguments) {
    use fmt::Write;

    // Save DAIF and mask IRQs to prevent reentrant access from ISRs.
    let daif: u64;
    unsafe { core::arch::asm!("mrs {}, DAIF", out(reg) daif) };
    unsafe { core::arch::asm!("msr DAIFSet, #2") };

    // SAFETY: IRQs are masked, so no ISR can reenter this function.
    if let Some(writer) = unsafe { (*WRITER.0.get()).as_mut() } {
        let _ = writer.write_fmt(args);
    }

    // Restore previous IRQ mask state.
    if daif & (1 << 7) == 0 {
        unsafe { core::arch::asm!("msr DAIFClr, #2") };
    }
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
