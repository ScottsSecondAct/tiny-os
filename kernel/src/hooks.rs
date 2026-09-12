// Hook functions — weak defaults that applications can override.
//
// Each hook has a default behavior (spec section 11.2). Applications
// override by defining a function with the same name and signature.

use crate::kprintln;

pub fn os_hook_idle(_core: u8) {
    unsafe { core::arch::asm!("wfi") };
}

pub fn os_hook_stack_overflow(task_id: u8, name: &str) {
    kprintln!("HOOK: stack overflow task {}:{}", task_id, name);
}

pub fn os_hook_data_abort(task_id: u8, addr: u64, esr: u64) {
    kprintln!(
        "HOOK: data abort task {} addr={:#x} esr={:#x}",
        task_id,
        addr,
        esr
    );
}

pub fn os_hook_hard_fault(esr: u64, elr: u64, far: u64) {
    kprintln!(
        "HOOK: hard fault esr={:#x} elr={:#x} far={:#x}",
        esr,
        elr,
        far
    );
}

pub fn os_hook_assert(file: &str, line: u32) {
    kprintln!("HOOK: assertion failed {}:{}", file, line);
}

pub fn os_hook_task_create(task_id: u8, name: &str) {
    let _ = (task_id, name);
}

pub fn os_hook_task_switch(from_id: u8, to_id: u8, core: u8) {
    let _ = (from_id, to_id, core);
}

pub fn os_hook_budget_overrun(task_id: u8, name: &str) {
    kprintln!("HOOK: budget overrun task {}:{}", task_id, name);
}

pub fn os_hook_deadline_miss(task_id: u8, name: &str) {
    kprintln!("HOOK: deadline miss task {}:{}", task_id, name);
}

pub fn os_hook_task_terminated(task_id: u8, name: &str, reason: u32) {
    kprintln!(
        "HOOK: task terminated {}:{} reason={}",
        task_id,
        name,
        reason
    );
}

pub fn os_hook_watchdog_expired() {
    kprintln!("HOOK: watchdog expired — initiating shutdown");
}

pub fn os_hook_health_check_failed(check: &str) {
    kprintln!("HOOK: health check failed: {}", check);
}

pub fn os_hook_shutdown(reason: &str) {
    kprintln!("HOOK: shutdown reason={}", reason);
}

pub fn os_hook_safety_critical_lost(task_id: u8, name: &str) {
    kprintln!("HOOK: safety-critical task lost {}:{}", task_id, name);
}
