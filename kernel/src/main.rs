#![no_std]
#![no_main]

mod panic;
pub mod print;

mod exceptions;
mod shell;

use arch::aarch64::{exceptions as exc, gic, timer};
use arch::uart::UartDriver;
use bsp::PlatformUart;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    print::init(uart);

    kprintln!("tiny_os Phase 2 boot");
    kprintln!("AArch64 EL1 | no_std | no_main");

    gic::init(bsp::GIC_DIST_BASE, bsp::GIC_CPU_BASE);
    exc::register_irq(timer::TIMER_IRQ_ID, timer::handle_tick);
    gic::set_priority(timer::TIMER_IRQ_ID, 0x80);
    gic::enable(timer::TIMER_IRQ_ID);
    timer::init(1000);

    unsafe { core::arch::asm!("msr daifclr, #2") };

    let start = timer::read_counter();
    let freq = timer::frequency();
    let target = start + freq / 4;
    while timer::read_counter() < target {
        core::hint::spin_loop();
    }
    let ticks = exc::tick_count();
    kprintln!("timer: {} Hz, {} ticks in 250ms (expect ~250)", freq, ticks);
    kprintln!("UART: 115200 8N1");
    kprintln!("type 'help' for commands");

    let mut shell_uart = PlatformUart::new();
    shell_uart.init();
    shell::run(&mut shell_uart);
}
