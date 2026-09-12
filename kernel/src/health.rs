use crate::mm::pool;
use crate::{audit, hooks, integrity, klog_info, klog_warn, os_cfg, sched, watchdog};

const HEALTH_INTERVAL_MS: u32 = os_cfg::HEALTH_CHECK_INTERVAL_MS;
const STACK_WARN_PERCENT: usize = os_cfg::STACK_WARN_PERCENT;

pub fn health_task(_arg: usize) -> ! {
    loop {
        check_stacks();
        check_utilization();
        check_watchdog();
        check_ready_queue();
        check_mutex_ownership();
        check_tick_monotonicity();
        check_pool_accounting();
        check_code_integrity();
        sched::delay(HEALTH_INTERVAL_MS);
    }
}

fn check_stacks() {
    let info = sched::task_stack_info();
    let mut max_pct: usize = 0;
    let mut task_count: u8 = 0;

    for &(id, name, used, size) in info.iter() {
        if size == 0 {
            continue;
        }
        task_count += 1;
        let pct = (used * 100).checked_div(size).unwrap_or(0);
        if pct > max_pct {
            max_pct = pct;
        }
        if pct >= STACK_WARN_PERCENT {
            klog_warn!(
                "health",
                "task '{}' (id={}) stack {}% ({}/{})",
                name,
                id,
                pct,
                used,
                size
            );
            hooks::os_hook_stack_overflow(id, name);
        }
    }

    let (busy, total) = sched::utilization();
    let cpu_pct = (busy * 100).checked_div(total).unwrap_or(0) as u32;
    klog_info!(
        "health",
        "{} tasks ok, max-stack {}%, cpu {}%",
        task_count,
        max_pct,
        cpu_pct
    );
}

fn check_utilization() {
    let (busy, total) = sched::utilization();
    if total > 5000 {
        let cpu_pct = (busy * 100) / total;
        if cpu_pct > 95 {
            klog_warn!("health", "high CPU utilization: {}%", cpu_pct);
        }
    }
}

fn check_watchdog() {
    if !watchdog::is_enabled() {
        klog_warn!("health", "watchdog is disabled");
    }
}

fn check_ready_queue() {
    if !sched::check_ready_queue_integrity() {
        klog_warn!("health", "ready queue integrity check FAILED");
        hooks::os_hook_health_check_failed("ready_queue");
    }
}

fn check_mutex_ownership() {
    if !sched::check_mutex_ownership() {
        klog_warn!("health", "mutex ownership check FAILED");
        hooks::os_hook_health_check_failed("mutex_ownership");
    }
}

fn check_tick_monotonicity() {
    if !sched::check_tick_monotonicity() {
        klog_warn!("health", "tick monotonicity check FAILED");
        hooks::os_hook_health_check_failed("tick_monotonicity");
    }
}

fn check_pool_accounting() {
    let count = pool::pool_count();
    for i in 0..count {
        if let Some((total, free, _blk_size)) = pool::pool_info(i) {
            if free > total {
                klog_warn!(
                    "health",
                    "pool {} accounting mismatch: free={} > total={}",
                    i,
                    free,
                    total
                );
                hooks::os_hook_health_check_failed("pool_accounting");
            }
        }
    }
}

fn check_code_integrity() {
    if integrity::verify() {
        audit::log(audit::AuditEvent::IntegrityOk, ".text CRC ok");
    } else {
        klog_warn!("health", "CODE INTEGRITY FAILURE — .text CRC mismatch!");
        audit::log(audit::AuditEvent::IntegrityFail, ".text CRC mismatch");
        hooks::os_hook_health_check_failed("code_integrity");
    }
}
