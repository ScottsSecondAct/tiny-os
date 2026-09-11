use crate::{klog_warn, sched};
use crate::sched::Criticality;
use core::sync::atomic::{AtomicBool, Ordering};

static ELEVATED: AtomicBool = AtomicBool::new(false);

pub fn os_criticality_switch(min_level: Criticality) {
    if ELEVATED.swap(true, Ordering::SeqCst) {
        return;
    }
    klog_warn!("crit", "criticality switch: suspending tasks below {:?}", min_level);
    let info = sched::task_list_ext();
    for &(id, _name, _prio, state, crit, _budget, _run) in info.iter() {
        if state == sched::TaskState::Dormant || id == 0 {
            continue;
        }
        if criticality_rank(crit) < criticality_rank(min_level) {
            sched::task_suspend(id);
        }
    }
}

pub fn os_criticality_restore() {
    if !ELEVATED.swap(false, Ordering::SeqCst) {
        return;
    }
    klog_warn!("crit", "criticality restore: resuming suspended tasks");
    let info = sched::task_list_ext();
    for &(id, _name, _prio, state, _crit, _budget, _run) in info.iter() {
        if state == sched::TaskState::Suspended && id != 0 {
            sched::task_resume(id);
        }
    }
}

pub fn is_elevated() -> bool {
    ELEVATED.load(Ordering::Relaxed)
}

fn criticality_rank(c: Criticality) -> u8 {
    match c {
        Criticality::SafetyCritical => 3,
        Criticality::MissionCritical => 2,
        Criticality::Standard => 1,
        Criticality::BestEffort => 0,
    }
}
