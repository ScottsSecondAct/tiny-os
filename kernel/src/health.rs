use crate::{klog_info, klog_warn, sched, watchdog};

const HEALTH_INTERVAL_MS: u32 = 5000;
const STACK_WARN_PERCENT: usize = 80;

pub fn health_task(_arg: usize) -> ! {
    loop {
        check_stacks();
        check_utilization();
        check_watchdog();
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
        let pct = if size > 0 { (used * 100) / size } else { 0 };
        if pct > max_pct {
            max_pct = pct;
        }
        if pct >= STACK_WARN_PERCENT {
            klog_warn!("health", "task '{}' (id={}) stack {}% ({}/{})", name, id, pct, used, size);
        }
    }

    let (busy, total) = sched::utilization();
    let cpu_pct = if total > 0 { ((busy * 100) / total) as u32 } else { 0 };
    klog_info!("health", "{} tasks ok, max-stack {}%, cpu {}%", task_count, max_pct, cpu_pct);
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
