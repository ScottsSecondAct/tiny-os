#![no_std]
#![no_main]

mod panic;
pub mod print;

use arch::uart::UartDriver;
use bsp::PlatformUart;

/// Kernel entry point. Called by `_start` in `arch/src/aarch64/boot.S`
/// after the BSS is zeroed and the stack pointer is set.
///
/// This function must never return; boot.S falls through to a WFE spin if
/// it does, but `-> !` makes the contract explicit to the compiler.
#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    print::init(uart);

    kprintln!("tiny_os Phase 1 boot");
    kprintln!("AArch64 EL1 | no_std | no_main");
    kprintln!("UART: 115200 8N1");
    kprintln!("Halting (Phase 2 will add interrupts)");

    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)) };
    }
}
