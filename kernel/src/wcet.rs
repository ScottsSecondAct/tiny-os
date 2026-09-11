// WCET measurement harness using the AArch64 PMU cycle counter.
//
// PMCCNTR_EL0 counts CPU cycles at the core frequency. On Cortex-A76
// at 2.4 GHz, 1 cycle = ~0.42 ns, so 2400 cycles = 1 µs.

use crate::os_cfg;

const MAX_MEASUREMENTS: usize = 32;

#[derive(Clone, Copy)]
pub struct WcetRecord {
    pub name: &'static str,
    pub min_cycles: u64,
    pub max_cycles: u64,
    pub last_cycles: u64,
    pub count: u64,
    pub total_cycles: u64,
}

impl WcetRecord {
    const fn empty() -> Self {
        Self {
            name: "",
            min_cycles: u64::MAX,
            max_cycles: 0,
            last_cycles: 0,
            count: 0,
            total_cycles: 0,
        }
    }

    fn update(&mut self, cycles: u64) {
        if cycles < self.min_cycles { self.min_cycles = cycles; }
        if cycles > self.max_cycles { self.max_cycles = cycles; }
        self.last_cycles = cycles;
        self.count += 1;
        self.total_cycles += cycles;
    }

    pub fn avg_cycles(&self) -> u64 {
        if self.count > 0 { self.total_cycles / self.count } else { 0 }
    }
}

struct WcetTable {
    records: [WcetRecord; MAX_MEASUREMENTS],
    count: usize,
}

use core::cell::UnsafeCell;
struct WcetCell(UnsafeCell<WcetTable>);
unsafe impl Sync for WcetCell {}

static TABLE: WcetCell = WcetCell(UnsafeCell::new(WcetTable {
    records: [WcetRecord::empty(); MAX_MEASUREMENTS],
    count: 0,
}));

fn table() -> &'static mut WcetTable {
    // SAFETY: WCET measurements are done under DAIF mask or spinlock.
    unsafe { &mut *TABLE.0.get() }
}

pub fn enable_cycle_counter() {
    unsafe {
        // Enable user-mode access to cycle counter (not strictly needed for EL1).
        core::arch::asm!("msr pmcr_el0, {}", in(reg) 1u64);
        // Enable cycle counter (bit 31 of PMCNTENSET_EL0).
        core::arch::asm!("msr pmcntenset_el0, {}", in(reg) 1u64 << 31);
    }
}

#[inline(always)]
pub fn read_cycles() -> u64 {
    let val: u64;
    unsafe { core::arch::asm!("mrs {}, pmccntr_el0", out(reg) val) };
    val
}

pub fn register_service(name: &'static str) -> usize {
    let t = table();
    if t.count >= MAX_MEASUREMENTS {
        return MAX_MEASUREMENTS - 1;
    }
    let idx = t.count;
    t.records[idx].name = name;
    t.count += 1;
    idx
}

pub fn record(idx: usize, cycles: u64) {
    let t = table();
    if idx < t.count {
        t.records[idx].update(cycles);
    }
}

pub fn get_record(idx: usize) -> Option<WcetRecord> {
    let t = table();
    if idx < t.count && t.records[idx].count > 0 {
        Some(t.records[idx])
    } else {
        None
    }
}

pub fn record_count() -> usize {
    table().count
}

pub fn dump_all() {
    let t = table();
    if t.count == 0 {
        crate::kprintln!("(no WCET records)");
        return;
    }
    crate::kprintln!("{:<24} {:>10} {:>10} {:>10} {:>8}", "Service", "Min(cy)", "Max(cy)", "Avg(cy)", "Count");
    for i in 0..t.count {
        let r = &t.records[i];
        if r.count == 0 { continue; }
        crate::kprintln!("{:<24} {:>10} {:>10} {:>10} {:>8}",
            r.name,
            r.min_cycles,
            r.max_cycles,
            r.avg_cycles(),
            r.count,
        );
    }
}

// Spec section 5.4: WCET bounds for 17 kernel services.
// These are reference values at 2.4 GHz (Cortex-A76).
pub const WCET_CONTEXT_SWITCH_SAME_ASID_NS: u64 = 1000;
pub const WCET_CONTEXT_SWITCH_DIFF_ASID_NS: u64 = 2000;
pub const WCET_SCHEDULER_DISPATCH_NS: u64 = 200;
pub const WCET_MUTEX_LOCK_NS: u64 = 500;
pub const WCET_MUTEX_UNLOCK_NS: u64 = 2000;
pub const WCET_SEM_WAIT_NS: u64 = 500;
pub const WCET_SEM_POST_NS: u64 = 300;
pub const WCET_QUEUE_SEND_NS: u64 = 300;
pub const WCET_QUEUE_RECEIVE_NS: u64 = 300;
pub const WCET_POOL_ALLOC_NS: u64 = 100;
pub const WCET_POOL_FREE_NS: u64 = 100;

// Helper: convert nanoseconds to cycles at a given frequency in MHz.
pub const fn ns_to_cycles(ns: u64, freq_mhz: u64) -> u64 {
    (ns * freq_mhz) / 1000
}
