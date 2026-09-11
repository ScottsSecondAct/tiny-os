#![no_std]
#![no_main]

mod panic;
pub mod print;
pub mod os_cfg;

pub mod criticality;
mod drivers;
mod exceptions;
pub mod fault_inject;
mod health;
pub mod hooks;
pub mod shutdown;
pub mod klog;
mod mm;
pub mod netbuf;
pub mod sched;
pub mod sched_analysis;
#[path = "../../examples/temp_monitor/main.rs"]
mod temp_monitor;
#[path = "../../examples/sensor_gateway/main.rs"]
mod sensor_gateway;
#[path = "../../examples/system_dashboard/main.rs"]
mod system_dashboard;
#[path = "../../examples/data_logger/main.rs"]
mod data_logger;
#[path = "../../examples/echo_server/main.rs"]
mod echo_server;
#[path = "../../examples/led_blinker/main.rs"]
mod led_blinker;
#[path = "../../examples/rate_limit_demo/main.rs"]
mod rate_limit_demo;
#[path = "../../examples/plc_motion/main.rs"]
mod plc_motion;
#[path = "../../examples/machine_vision/main.rs"]
mod machine_vision;
#[path = "../../examples/crypto_signer/main.rs"]
mod crypto_signer;
#[path = "../../examples/power_monitor/main.rs"]
mod power_monitor;
#[path = "../../examples/rtc_clock/main.rs"]
mod rtc_clock;
mod shell;
pub mod periph;
pub mod syscall;
pub mod spinlock;
pub mod fs;
pub mod net;
pub mod storage;
pub mod sync;
mod user_tasks;
pub mod watchdog;
pub mod wcet;
pub mod crypto;
pub mod integrity;
pub mod audit;
pub mod jtag;
pub mod pac;
pub mod power;
pub mod rtc;
#[cfg(feature = "dynamic-load")]
pub mod loader;

use arch::aarch64::{emmc2, exceptions as exc, gic, mailbox, mmu, timer, smp};
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
static mut TEMP_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut GW_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut DASH_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut DLOG_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut ECHO_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut LED_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut RLIM_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut PLC_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut MV_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut CRYPTO_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut PWRMON_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
static mut RTCCLK_KERNEL_STACK: TaskStack<8192> = TaskStack([0; 8192]);
#[repr(align(4096))]
struct UserStack([u8; 16384]);
static mut USER_STACK: UserStack = UserStack([0; 16384]);
static mut TEMP_USER_STACK: UserStack = UserStack([0; 16384]);
static mut GW_USER_STACK: UserStack = UserStack([0; 16384]);
static mut DASH_USER_STACK: UserStack = UserStack([0; 16384]);
static mut DLOG_USER_STACK: UserStack = UserStack([0; 16384]);
static mut ECHO_USER_STACK: UserStack = UserStack([0; 16384]);
static mut LED_USER_STACK: UserStack = UserStack([0; 16384]);
static mut RLIM_USER_STACK: UserStack = UserStack([0; 16384]);
static mut PLC_USER_STACK: UserStack = UserStack([0; 16384]);
static mut MV_USER_STACK: UserStack = UserStack([0; 16384]);
static mut CRYPTO_USER_STACK: UserStack = UserStack([0; 16384]);
static mut PWRMON_USER_STACK: UserStack = UserStack([0; 16384]);
static mut RTCCLK_USER_STACK: UserStack = UserStack([0; 16384]);

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

const WATCHDOG_TIMEOUT_MS: u32 = os_cfg::WDT_TIMEOUT_MS;
const WATCHDOG_KICK_INTERVAL_MS: u32 = os_cfg::WDT_KICK_INTERVAL_MS;

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

    kprintln!("tiny_os boot (Phase 14: Advanced Attack Hardening)");
    kprintln!("AArch64 EL1 | no_std | no_main");

    gic::init(bsp::GIC_DIST_BASE, bsp::GIC_CPU_BASE);
    exc::register_irq(timer::TIMER_IRQ_ID, timer::handle_tick);
    gic::set_priority(timer::TIMER_IRQ_ID, 0x80);
    gic::enable(timer::TIMER_IRQ_ID);
    timer::init(os_cfg::TICK_RATE_HZ);

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

    // Initialize VideoCore mailbox for temperature sensor and DVFS.
    mailbox::init(bsp::MAILBOX_BASE);

    // Initialize software RTC (monotonic clock-derived).
    kprintln!("rtc: software clock initialized");

    // Initialize power management (DVFS via mailbox).
    kprintln!("power: DVFS via VideoCore mailbox");

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

    // Initialize code integrity (CRC32 of .text section).
    integrity::init();

    // JTAG/debug port lockdown (safety-critical mode only).
    jtag::lockdown();

    // Pointer Authentication (ARMv8.3, bsp-rpi5 only).
    pac::init();

    // Initialize audit log and record boot event.
    audit::log(audit::AuditEvent::Boot, "kernel start");
    kprintln!("audit: 64-entry ring buffer");

    // Initialize klog subsystem.
    kprintln!("klog: {}-entry ring buffer, level={}", os_cfg::LOG_BUFFER_SIZE, klog::get_level().as_str());

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

    // Shared user code region for all EL0 tasks.
    extern "C" {
        static __user_text_start: u8;
        static __user_text_end: u8;
    }
    let code_base = unsafe { &raw const __user_text_start as usize };
    let code_size = unsafe { &raw const __user_text_end as usize - code_base };
    let user_stack_pages = 16384 / 4096;

    // Create EL0 user demo task.
    let user_entry = user_tasks::user_demo as *const () as usize;
    let user_stack_base = unsafe { &raw const USER_STACK.0 as usize };
    let user_stack_top = user_stack_base + 16384;

    let ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, user_stack_base, user_stack_pages)
    };

    let user_kernel_stack = unsafe { &mut USER_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "user-demo", 100, Criticality::Standard,
        user_kernel_stack, user_entry, user_stack_top, 0, ttbr0,
    ).expect("failed to create user demo task");

    // Create EL0 temperature monitor task (examples/temp_monitor.rs).
    let temp_entry = temp_monitor::temp_monitor_main as *const () as usize;
    let temp_stack_base = unsafe { &raw const TEMP_USER_STACK.0 as usize };
    let temp_stack_top = temp_stack_base + 16384;

    let temp_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, temp_stack_base, user_stack_pages)
    };

    let temp_kernel_stack = unsafe { &mut TEMP_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "temp-mon", 100, Criticality::Standard,
        temp_kernel_stack, temp_entry, temp_stack_top, 0, temp_ttbr0,
    ).expect("failed to create temp monitor task");

    // Create EL0 sensor gateway task (examples/sensor_gateway.rs).
    let gw_entry = sensor_gateway::sensor_gateway_main as *const () as usize;
    let gw_stack_base = unsafe { &raw const GW_USER_STACK.0 as usize };
    let gw_stack_top = gw_stack_base + 16384;

    let gw_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, gw_stack_base, user_stack_pages)
    };

    let gw_kernel_stack = unsafe { &mut GW_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "sensor-gw", 80, Criticality::MissionCritical,
        gw_kernel_stack, gw_entry, gw_stack_top, 0, gw_ttbr0,
    ).expect("failed to create sensor gateway task");

    // Create EL0 system dashboard task (examples/system_dashboard.rs).
    let dash_entry = system_dashboard::system_dashboard_main as *const () as usize;
    let dash_stack_base = unsafe { &raw const DASH_USER_STACK.0 as usize };
    let dash_stack_top = dash_stack_base + 16384;
    let dash_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, dash_stack_base, user_stack_pages)
    };
    let dash_kernel_stack = unsafe { &mut DASH_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "dashboard", 150, Criticality::Standard,
        dash_kernel_stack, dash_entry, dash_stack_top, 0, dash_ttbr0,
    ).expect("failed to create dashboard task");

    // Create EL0 data logger task (examples/data_logger.rs).
    let dlog_entry = data_logger::data_logger_main as *const () as usize;
    let dlog_stack_base = unsafe { &raw const DLOG_USER_STACK.0 as usize };
    let dlog_stack_top = dlog_stack_base + 16384;
    let dlog_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, dlog_stack_base, user_stack_pages)
    };
    let dlog_kernel_stack = unsafe { &mut DLOG_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "data-log", 120, Criticality::Standard,
        dlog_kernel_stack, dlog_entry, dlog_stack_top, 0, dlog_ttbr0,
    ).expect("failed to create data logger task");

    // Create EL0 echo server task (examples/echo_server.rs).
    let echo_entry = echo_server::echo_server_main as *const () as usize;
    let echo_stack_base = unsafe { &raw const ECHO_USER_STACK.0 as usize };
    let echo_stack_top = echo_stack_base + 16384;
    let echo_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, echo_stack_base, user_stack_pages)
    };
    let echo_kernel_stack = unsafe { &mut ECHO_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "echo-srv", 110, Criticality::Standard,
        echo_kernel_stack, echo_entry, echo_stack_top, 0, echo_ttbr0,
    ).expect("failed to create echo server task");

    // Create EL0 LED blinker task (examples/led_blinker.rs).
    let led_entry = led_blinker::led_blinker_main as *const () as usize;
    let led_stack_base = unsafe { &raw const LED_USER_STACK.0 as usize };
    let led_stack_top = led_stack_base + 16384;
    let led_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, led_stack_base, user_stack_pages)
    };
    let led_kernel_stack = unsafe { &mut LED_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "led-blink", 200, Criticality::Standard,
        led_kernel_stack, led_entry, led_stack_top, 0, led_ttbr0,
    ).expect("failed to create LED blinker task");

    // Create EL0 rate limit demo task (examples/rate_limit_demo.rs).
    let rlim_entry = rate_limit_demo::rate_limit_demo_main as *const () as usize;
    let rlim_stack_base = unsafe { &raw const RLIM_USER_STACK.0 as usize };
    let rlim_stack_top = rlim_stack_base + 16384;
    let rlim_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, rlim_stack_base, user_stack_pages)
    };
    let rlim_kernel_stack = unsafe { &mut RLIM_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "rate-demo", 180, Criticality::Standard,
        rlim_kernel_stack, rlim_entry, rlim_stack_top, 0, rlim_ttbr0,
    ).expect("failed to create rate limit demo task");

    // Create EL0 PLC motion controller task (examples/plc_motion.rs).
    let plc_entry = plc_motion::plc_motion_main as *const () as usize;
    let plc_stack_base = unsafe { &raw const PLC_USER_STACK.0 as usize };
    let plc_stack_top = plc_stack_base + 16384;
    let plc_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, plc_stack_base, user_stack_pages)
    };
    let plc_kernel_stack = unsafe { &mut PLC_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "plc-ctrl", 60, Criticality::MissionCritical,
        plc_kernel_stack, plc_entry, plc_stack_top, 0, plc_ttbr0,
    ).expect("failed to create PLC motion task");

    // Create EL0 machine vision inspector task (examples/machine_vision.rs).
    let mv_entry = machine_vision::machine_vision_main as *const () as usize;
    let mv_stack_base = unsafe { &raw const MV_USER_STACK.0 as usize };
    let mv_stack_top = mv_stack_base + 16384;
    let mv_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, mv_stack_base, user_stack_pages)
    };
    let mv_kernel_stack = unsafe { &mut MV_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "vision", 70, Criticality::MissionCritical,
        mv_kernel_stack, mv_entry, mv_stack_top, 0, mv_ttbr0,
    ).expect("failed to create machine vision task");

    // Create EL0 crypto signer task (examples/crypto_signer.rs).
    let crypto_entry = crypto_signer::crypto_signer_main as *const () as usize;
    let crypto_stack_base = unsafe { &raw const CRYPTO_USER_STACK.0 as usize };
    let crypto_stack_top = crypto_stack_base + 16384;
    let crypto_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, crypto_stack_base, user_stack_pages)
    };
    let crypto_kernel_stack = unsafe { &mut CRYPTO_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "crypto-sig", 130, Criticality::Standard,
        crypto_kernel_stack, crypto_entry, crypto_stack_top, 0, crypto_ttbr0,
    ).expect("failed to create crypto signer task");

    // Create EL0 power monitor task (examples/power_monitor.rs).
    let pwrmon_entry = power_monitor::power_monitor_main as *const () as usize;
    let pwrmon_stack_base = unsafe { &raw const PWRMON_USER_STACK.0 as usize };
    let pwrmon_stack_top = pwrmon_stack_base + 16384;
    let pwrmon_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, pwrmon_stack_base, user_stack_pages)
    };
    let pwrmon_kernel_stack = unsafe { &mut PWRMON_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "pwr-mon", 140, Criticality::Standard,
        pwrmon_kernel_stack, pwrmon_entry, pwrmon_stack_top, 0, pwrmon_ttbr0,
    ).expect("failed to create power monitor task");

    // Create EL0 RTC clock task (examples/rtc_clock.rs).
    let rtcclk_entry = rtc_clock::rtc_clock_main as *const () as usize;
    let rtcclk_stack_base = unsafe { &raw const RTCCLK_USER_STACK.0 as usize };
    let rtcclk_stack_top = rtcclk_stack_base + 16384;
    let rtcclk_ttbr0 = unsafe {
        mmu::create_user_page_table(code_base, code_size, rtcclk_stack_base, user_stack_pages)
    };
    let rtcclk_kernel_stack = unsafe { &mut RTCCLK_KERNEL_STACK.0[..] };
    sched::task_create_user(
        "rtc-clock", 160, Criticality::Standard,
        rtcclk_kernel_stack, rtcclk_entry, rtcclk_stack_top, 0, rtcclk_ttbr0,
    ).expect("failed to create RTC clock task");

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
