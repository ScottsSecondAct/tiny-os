use crate::{hooks, kprintln, os_cfg};
use core::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[repr(C)]
pub struct DiagRegion {
    pub esr: u64,
    pub elr: u64,
    pub far: u64,
    pub spsr: u64,
    pub regs: [u64; 31],
    pub tick: u64,
    pub core_id: u8,
    pub task_id: u8,
    pub valid: bool,
}

static mut DIAG: DiagRegion = DiagRegion {
    esr: 0,
    elr: 0,
    far: 0,
    spsr: 0,
    regs: [0; 31],
    tick: 0,
    core_id: 0,
    task_id: 0,
    valid: false,
};

pub fn structured_shutdown(reason: &str, esr: u64, elr: u64, far: u64) {
    if SHUTDOWN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        loop {
            unsafe { core::arch::asm!("wfe") };
        }
    }

    // Step 1: Mask all interrupts on this core.
    unsafe { core::arch::asm!("msr daifset, #0xF") };

    // Step 2: Save diagnostic state.
    let core = arch::aarch64::smp::core_id() as u8;
    let task_id = crate::sched::current_task_id();
    let tick = arch::aarch64::exceptions::tick_count();

    // SAFETY: Single writer (shutdown is one-shot).
    unsafe {
        DIAG.esr = esr;
        DIAG.elr = elr;
        DIAG.far = far;
        DIAG.tick = tick;
        DIAG.core_id = core;
        DIAG.task_id = task_id;
        DIAG.valid = true;
    }

    // Step 3: Log the fault.
    kprintln!("=== STRUCTURED SHUTDOWN ===");
    kprintln!("reason: {}", reason);
    kprintln!("core={} task={} tick={}", core, task_id, tick);
    kprintln!("ESR={:#018x} ELR={:#018x} FAR={:#018x}", esr, elr, far);

    // Step 4: Call shutdown hook.
    hooks::os_hook_shutdown(reason);

    // Step 5: Flush log entries (best-effort).
    crate::klog::flush();

    // Step 6: Reboot or halt.
    if os_cfg::REBOOT_ON_FAULT {
        kprintln!("rebooting...");
        reboot();
    } else {
        kprintln!("system halted (WFE)");
        halt_all_cores();
    }
}

pub fn diag_region() -> &'static DiagRegion {
    // SAFETY: Read-only after shutdown completes.
    unsafe { &*core::ptr::addr_of!(DIAG) }
}

fn halt_all_cores() -> ! {
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}

fn reboot() -> ! {
    // Use the PM watchdog for a hard reset (BCM2712 PM_WDOG).
    // Fallback: just halt if we can't access the watchdog.
    kprintln!("PM watchdog reboot not available, halting");
    halt_all_cores();
}
