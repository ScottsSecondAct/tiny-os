#![no_std]
#![no_main]

mod panic;
pub mod print;

mod exceptions;
mod mm;
pub mod sched;
mod shell;

use arch::aarch64::{exceptions as exc, gic, timer};
use arch::uart::UartDriver;
use bsp::PlatformUart;

#[repr(align(16))]
struct TaskStack<const N: usize>([u8; N]);

static mut SHELL_STACK: TaskStack<16384> = TaskStack([0; 16384]);
static mut DEMO_STACK_A: TaskStack<8192> = TaskStack([0; 8192]);
static mut DEMO_STACK_B: TaskStack<8192> = TaskStack([0; 8192]);

fn shell_task(_arg: usize) -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    shell::run(&mut uart);
}

fn demo_task_a(_arg: usize) -> ! {
    let mut counter: u64 = 0;
    loop {
        counter += 1;
        kprintln!("[task-a] count={}", counter);
        sched::delay(2000);
    }
}

fn demo_task_b(_arg: usize) -> ! {
    let mut counter: u64 = 0;
    loop {
        counter += 1;
        kprintln!("[task-b] count={}", counter);
        sched::delay(3000);
    }
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    print::init(uart);

    kprintln!("tiny_os Phase 4 boot");
    kprintln!("AArch64 EL1 | no_std | no_main");

    gic::init(bsp::GIC_DIST_BASE, bsp::GIC_CPU_BASE);
    exc::register_irq(timer::TIMER_IRQ_ID, timer::handle_tick);
    gic::set_priority(timer::TIMER_IRQ_ID, 0x80);
    gic::enable(timer::TIMER_IRQ_ID);
    timer::init(1000);

    unsafe { core::arch::asm!("msr daifclr, #2") };

    mm::init();

    // Verify timer accuracy before starting the scheduler.
    let freq = timer::frequency();
    let t0 = exc::tick_count();
    let start = timer::read_counter();
    let target = start + freq / 4;
    while timer::read_counter() < target {
        core::hint::spin_loop();
    }
    let ticks = exc::tick_count() - t0;
    kprintln!("timer: {} Hz, {} ticks in 250ms (expect ~250)", freq, ticks);

    // Initialize the scheduler (creates idle task).
    sched::init();

    // Create the shell task at priority 10.
    let shell_stack = unsafe { &mut SHELL_STACK.0[..] };
    sched::task_create("shell", 10, shell_stack, shell_task, 0)
        .expect("failed to create shell task");

    // Create demo tasks at lower priority.
    let demo_a = unsafe { &mut DEMO_STACK_A.0[..] };
    sched::task_create("demo-a", 200, demo_a, demo_task_a, 0)
        .expect("failed to create demo-a");

    let demo_b = unsafe { &mut DEMO_STACK_B.0[..] };
    sched::task_create("demo-b", 200, demo_b, demo_task_b, 0)
        .expect("failed to create demo-b");

    kprintln!("sched: {} tasks created", sched::task_count());
    kprintln!("type 'help' for commands");

    // Start the scheduler — this does not return.
    sched::start();
}
