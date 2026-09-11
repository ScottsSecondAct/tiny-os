#![no_std]
#![no_main]

mod panic;
pub mod print;

mod drivers;
mod exceptions;
mod health;
pub mod klog;
mod mm;
pub mod netbuf;
pub mod sched;
mod shell;
pub mod syscall;
pub mod spinlock;
pub mod fs;
pub mod net;
pub mod storage;
pub mod sync;
mod user_tasks;
pub mod watchdog;

use arch::aarch64::{emmc2, exceptions as exc, gic, mmu, timer, smp};
use arch::uart::UartDriver;
use bsp::PlatformUart;
use sched::Criticality;
use sync::mutex::{Mutex, MutexProtocol};
use sync::semaphore::Semaphore;
use core::sync::atomic::{AtomicU8, Ordering};

const NUM_SECONDARY_CORES: usize = 3;

#[repr(align(16))]
struct TaskStack<const N: usize>([u8; N]);

static mut SHELL_STACK: TaskStack<16384> = TaskStack([0; 16384]);
static mut DEMO_STACK_A: TaskStack<8192> = TaskStack([0; 8192]);
static mut DEMO_STACK_B: TaskStack<8192> = TaskStack([0; 8192]);
static mut WATCHDOG_STACK: TaskStack<4096> = TaskStack([0; 4096]);
static mut HEALTH_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut NET_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut USER_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
#[repr(align(4096))]
struct UserStack([u8; 16384]);
static mut USER_STACK: UserStack = UserStack([0; 16384]);

// Per-secondary-core boot stacks (referenced by boot.S via SECONDARY_STACKS).
#[repr(align(16))]
struct SecondaryStacks([[u8; 8192]; smp::MAX_CORES]);
#[no_mangle]
static mut SECONDARY_STACKS: SecondaryStacks = SecondaryStacks([[0; 8192]; smp::MAX_CORES]);

static SHARED_MUTEX: Mutex = Mutex::new(MutexProtocol::PriorityInheritance);
struct SyncU64(core::cell::UnsafeCell<u64>);
// SAFETY: Access protected by SHARED_MUTEX.
unsafe impl Sync for SyncU64 {}
static SHARED_COUNTER: SyncU64 = SyncU64(core::cell::UnsafeCell::new(0));

static SEM_SIGNAL: Semaphore = Semaphore::binary(0);

const WATCHDOG_TIMEOUT_MS: u32 = 5000;
const WATCHDOG_KICK_INTERVAL_MS: u32 = 2000;

/// Tracks how many secondary cores have finished init.
static CORES_ONLINE: AtomicU8 = AtomicU8::new(1);

fn shell_task(_arg: usize) -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    shell::run(&mut uart);
}

fn demo_task_a(_arg: usize) -> ! {
    loop {
        SHARED_MUTEX.lock().expect("lock failed");
        // SAFETY: Protected by SHARED_MUTEX.
        let counter = unsafe { &mut *SHARED_COUNTER.0.get() };
        *counter += 1;
        let val = *counter;
        SHARED_MUTEX.unlock().expect("unlock failed");

        kprintln!("[task-a@core{}] counter={}", smp::core_id(), val);
        SEM_SIGNAL.post().ok();
        sched::delay(2000);
    }
}

fn demo_task_b(_arg: usize) -> ! {
    loop {
        SEM_SIGNAL.wait().expect("sem wait failed");

        SHARED_MUTEX.lock().expect("lock failed");
        // SAFETY: Protected by SHARED_MUTEX.
        let val = unsafe { *SHARED_COUNTER.0.get() };
        SHARED_MUTEX.unlock().expect("unlock failed");

        kprintln!("[task-b@core{}] saw counter={}", smp::core_id(), val);
        sched::delay(500);
    }
}

fn watchdog_kick_task(_arg: usize) -> ! {
    loop {
        watchdog::kick();
        sched::delay(WATCHDOG_KICK_INTERVAL_MS);
    }
}

/// Entry point for secondary cores. Called from boot.S → secondary_boot → bl secondary_main.
/// At this point: EL1, per-core stack set up, FP/SIMD enabled, vectors installed.
#[no_mangle]
pub extern "C" fn secondary_main(core_id: usize) -> ! {
    // Enable MMU using page tables built by the primary core.
    unsafe { mmu::init_secondary() };

    // Initialize this core's GIC CPU interface.
    gic::init_cpu_interface();

    // Enable the virtual timer PPI on this core.
    gic::set_priority(timer::TIMER_IRQ_ID, 0x80);
    gic::enable(timer::TIMER_IRQ_ID);
    timer::init_secondary();

    // Enable IRQs.
    unsafe { core::arch::asm!("msr daifclr, #2") };

    kprintln!("core {}: online", core_id);
    CORES_ONLINE.fetch_add(1, Ordering::Release);

    // Start the scheduler on this core (does not return).
    sched::start_secondary(core_id);
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    let mut uart = PlatformUart::new();
    uart.init();
    print::init(uart);

    kprintln!("tiny_os Phase 10 boot (Networking & User Mode)");
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

    // Initialize NetBuf DMA buffer pool.
    netbuf::init();
    let (nb_total, nb_free) = netbuf::pool_stats();
    kprintln!("netbuf: {} buffers ({} free)", nb_total, nb_free);

    // Try EMMC2 SDHCI first; fall back to ramdisk for QEMU.
    match emmc2::init(bsp::EMMC2_BASE) {
        Ok(()) => {
            if let Some((sdhc, blocks)) = emmc2::card_info() {
                let size_mb = blocks / 2048;
                kprintln!(
                    "sd: {} card, {} blocks ({} MB)",
                    if sdhc { "SDHC" } else { "SDSC" },
                    blocks,
                    size_mb
                );
            }
        }
        Err(_) => {
            kprintln!("sd: no card detected, using ramdisk");
        }
    }

    // Initialize storage layer (picks EMMC2 or ramdisk).
    storage::init();
    storage::print_mbr_info();

    // Mount FAT32 filesystem.
    match fs::init() {
        Ok(()) => {
            kprintln!("fs: FAT32 mounted, {} clusters", fs::cluster_count());
        }
        Err(_) => {
            kprintln!("fs: no FAT32 partition found");
        }
    }

    // Initialize network stack with loopback device.
    let lo_mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    net::init(net::Ipv4Addr([127, 0, 0, 1]), lo_mac);
    kprintln!("net: loopback, IP 127.0.0.1");

    // Initialize klog subsystem.
    kprintln!("klog: {}-entry ring buffer, level={}", 64, klog::get_level().as_str());

    // Initialize watchdog.
    watchdog::init(WATCHDOG_TIMEOUT_MS);
    kprintln!("watchdog: enabled, {}ms timeout", WATCHDOG_TIMEOUT_MS);

    // Initialize the scheduler (creates idle task for core 0).
    sched::init();

    // Create the shell task at priority 10.
    let shell_stack = unsafe { &mut SHELL_STACK.0[..] };
    sched::task_create("shell", 10, Criticality::Standard, shell_stack, shell_task, 0)
        .expect("failed to create shell task");

    // Create demo tasks.
    let demo_a = unsafe { &mut DEMO_STACK_A.0[..] };
    sched::task_create("demo-a", 200, Criticality::Standard, demo_a, demo_task_a, 0)
        .expect("failed to create demo-a");

    let demo_b = unsafe { &mut DEMO_STACK_B.0[..] };
    sched::task_create("demo-b", 200, Criticality::Standard, demo_b, demo_task_b, 0)
        .expect("failed to create demo-b");

    // Create watchdog kick task at highest priority.
    let wdog_stack = unsafe { &mut WATCHDOG_STACK.0[..] };
    sched::task_create("wdog-kick", 0, Criticality::SafetyCritical, wdog_stack, watchdog_kick_task, 0)
        .expect("failed to create watchdog kick task");

    // Create health monitor task.
    let health_stack = unsafe { &mut HEALTH_STACK.0[..] };
    sched::task_create("health-mon", 1, Criticality::MissionCritical, health_stack, health::health_task, 0)
        .expect("failed to create health monitor task");

    // Create network task.
    let net_stack = unsafe { &mut NET_STACK.0[..] };
    sched::task_create("net", 5, Criticality::Standard, net_stack, net::net_task, 0)
        .expect("failed to create net task");

    // Create EL0 user demo task.
    let user_entry = user_tasks::user_demo as *const () as usize;
    let user_stack_base = unsafe { &raw const USER_STACK.0 as usize };
    let user_stack_size = 16384;
    let user_stack_top = user_stack_base + user_stack_size;
    let user_stack_pages = user_stack_size / 4096;

    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    let code_base = unsafe { &raw const __user_text_start as usize };
    let code_size = unsafe { &raw const __user_text_end as usize - code_base };

    let ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, user_stack_base, user_stack_pages)
    };

    let user_kernel_stack = unsafe { &mut USER_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "user-demo", 100, Criticality::Standard,
        user_kernel_stack, user_entry, user_stack_top, 0, ttbr0,
    ).expect("failed to create user demo task");

    kprintln!("sched: {} tasks created on core 0", sched::task_count());

    // Wake secondary cores.
    kprintln!("smp: waking {} secondary cores...", NUM_SECONDARY_CORES);
    for core_id in 1..=NUM_SECONDARY_CORES {
        smp::start_core(core_id);
    }

    // Wait for all secondary cores to come online (with timeout for QEMU).
    let smp_deadline = timer::read_counter() + timer::frequency() * 3;
    while CORES_ONLINE.load(Ordering::Acquire) < (NUM_SECONDARY_CORES + 1) as u8 {
        if timer::read_counter() > smp_deadline {
            kprintln!("smp: timeout ({} of {} cores)",
                CORES_ONLINE.load(Ordering::Relaxed), NUM_SECONDARY_CORES + 1);
            break;
        }
        core::hint::spin_loop();
    }
    let online = CORES_ONLINE.load(Ordering::Relaxed) as usize;
    if online >= NUM_SECONDARY_CORES + 1 {
        kprintln!("smp: all {} cores online", online);
    }

    kprintln!("type 'help' for commands");

    // Start the scheduler on core 0 — this does not return.
    sched::start();
}
