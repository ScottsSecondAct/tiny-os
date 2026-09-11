// Rate-Monotonic Analysis (RMA) and Response-Time Analysis (RTA).
//
// RMA: checks utilization bound U ≤ n*(2^(1/n) - 1).
// RTA: iterative response-time calculation with PIP blocking.

use crate::os_cfg;

const MAX_TASKSET: usize = os_cfg::MAX_TASKS;

#[derive(Clone, Copy)]
pub struct TaskParams {
    pub id: u8,
    pub period: u32,
    pub wcet: u32,
    pub blocking: u32,
    pub priority: u8,
}

impl TaskParams {
    pub const fn empty() -> Self {
        Self { id: 0, period: 0, wcet: 0, blocking: 0, priority: 255 }
    }
}

pub struct AnalysisResult {
    pub schedulable: bool,
    pub utilization_pct: u32,
    pub utilization_bound_pct: u32,
    pub response_times: [u32; MAX_TASKSET],
    pub task_count: usize,
}

pub fn rma_utilization_check(tasks: &[TaskParams]) -> (bool, u32, u32) {
    if tasks.is_empty() {
        return (true, 0, 100);
    }

    let n = tasks.len();
    // Calculate U = sum(Ci/Ti) * 1000 (fixed-point, per-mille)
    let mut u_permille: u64 = 0;
    for t in tasks {
        if t.period > 0 {
            u_permille += (t.wcet as u64 * 1000) / t.period as u64;
        }
    }

    // Liu & Layland bound: n * (2^(1/n) - 1)
    // For small n, use precomputed table (×1000 for per-mille).
    let bound_permille = match n {
        1 => 1000,
        2 => 828,
        3 => 780,
        4 => 757,
        5 => 743,
        6 => 735,
        7 => 729,
        8 => 724,
        _ => 693, // converges to ln(2) ≈ 0.693
    };

    let schedulable = u_permille <= bound_permille;
    let u_pct = ((u_permille * 100) / 1000) as u32;
    let bound_pct = ((bound_permille as u64 * 100) / 1000) as u32;

    (schedulable, u_pct, bound_pct)
}

pub fn response_time_analysis(tasks: &[TaskParams]) -> AnalysisResult {
    let n = tasks.len().min(MAX_TASKSET);
    let mut result = AnalysisResult {
        schedulable: true,
        utilization_pct: 0,
        utilization_bound_pct: 0,
        response_times: [0; MAX_TASKSET],
        task_count: n,
    };

    let (sched, u_pct, bound_pct) = rma_utilization_check(&tasks[..n]);
    result.utilization_pct = u_pct;
    result.utilization_bound_pct = bound_pct;

    // Sort tasks by priority (lower number = higher priority).
    let mut sorted: [TaskParams; MAX_TASKSET] = [TaskParams::empty(); MAX_TASKSET];
    for i in 0..n {
        sorted[i] = tasks[i];
    }
    for i in 0..n {
        for j in (i + 1)..n {
            if sorted[j].priority < sorted[i].priority {
                let tmp = sorted[i];
                sorted[i] = sorted[j];
                sorted[j] = tmp;
            }
        }
    }

    // RTA: for each task i, compute Ri iteratively.
    // Ri = Ci + Bi + sum_j>i(ceil(Ri/Tj) * Cj) for all higher-priority tasks j
    for i in 0..n {
        let ci = sorted[i].wcet;
        let bi = sorted[i].blocking;
        let ti = sorted[i].period;

        if ti == 0 {
            result.response_times[i] = 0;
            continue;
        }

        let mut r = ci + bi;
        let mut converged = false;

        for _iter in 0..100 {
            let mut interference: u64 = 0;
            for j in 0..i {
                if sorted[j].period > 0 {
                    let ceil = (r as u64 + sorted[j].period as u64 - 1) / sorted[j].period as u64;
                    interference += ceil * sorted[j].wcet as u64;
                }
            }

            let r_new = ci as u64 + bi as u64 + interference;
            if r_new > ti as u64 {
                result.schedulable = false;
                result.response_times[i] = r_new as u32;
                break;
            }

            if r_new as u32 == r {
                converged = true;
                result.response_times[i] = r;
                break;
            }
            r = r_new as u32;
        }

        if !converged && result.schedulable {
            result.schedulable = false;
            result.response_times[i] = r;
        }
    }

    result
}

pub fn dump_analysis(tasks: &[TaskParams]) {
    let result = response_time_analysis(tasks);
    crate::kprintln!("=== Schedulability Analysis ===");
    crate::kprintln!("Utilization: {}% (bound: {}%)", result.utilization_pct, result.utilization_bound_pct);
    crate::kprintln!("Schedulable: {}", if result.schedulable { "YES" } else { "NO" });
    for i in 0..result.task_count {
        crate::kprintln!("  Task {} (prio={}, C={}, T={}, B={}): R={}",
            tasks[i].id, tasks[i].priority,
            tasks[i].wcet, tasks[i].period, tasks[i].blocking,
            result.response_times[i]);
    }
}
