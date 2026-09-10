// AArch64 MMU setup for tiny_os.
//
// Identity mapping using 4KB granule with 2MB block descriptors at Level 2.
// Three MAIR indices: Device-nGnRnE (0), Normal WB Cacheable (1), Normal NC (2).

const PAGE_SIZE: usize = 4096;
const BLOCK_SIZE_2M: usize = 2 * 1024 * 1024;
const ENTRIES_PER_TABLE: usize = 512;

// Page table entry types.
const PT_BLOCK: u64 = 0b01;
const PT_TABLE: u64 = 0b11;

// Attribute fields for block descriptors.
const AF: u64 = 1 << 10;
const SH_INNER: u64 = 3 << 8;
const AP_RW_EL1: u64 = 0 << 6;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

const fn attr_idx(idx: u64) -> u64 {
    (idx & 7) << 2
}

// Block descriptor templates.
const NORMAL_RAM: u64 = PT_BLOCK | attr_idx(1) | AF | SH_INNER | AP_RW_EL1 | UXN;
const DEVICE_MEM: u64 = PT_BLOCK | attr_idx(0) | AF | AP_RW_EL1 | PXN | UXN;

// MAIR_EL1: index 0 = Device-nGnRnE, index 1 = Normal WB RA/WA, index 2 = Normal NC.
const MAIR_VALUE: u64 = 0x44_FF_00;

// TCR_EL1 fields.
const TCR_T0SZ: u64 = 16;
const TCR_IRGN0_WB_WA: u64 = 1 << 8;
const TCR_ORGN0_WB_WA: u64 = 1 << 10;
const TCR_SH0_INNER: u64 = 3 << 12;
const TCR_TG0_4K: u64 = 0 << 14;
const TCR_T1SZ: u64 = 16 << 16;
const TCR_EPD1: u64 = 1 << 23;
const TCR_TG1_4K: u64 = 2 << 30;
const TCR_IPS_40BIT: u64 = 2 << 32;

const TCR_VALUE: u64 = TCR_T0SZ
    | TCR_IRGN0_WB_WA
    | TCR_ORGN0_WB_WA
    | TCR_SH0_INNER
    | TCR_TG0_4K
    | TCR_T1SZ
    | TCR_EPD1
    | TCR_TG1_4K
    | TCR_IPS_40BIT;

#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; ENTRIES_PER_TABLE],
}

impl PageTable {
    const fn zero() -> Self {
        Self {
            entries: [0; ENTRIES_PER_TABLE],
        }
    }
}

// Static translation tables in BSS.
static mut L0: PageTable = PageTable::zero();
static mut L1: PageTable = PageTable::zero();
static mut L2: [PageTable; 8] = [
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
    PageTable::zero(),
];

static mut L2_NEXT: usize = 0;

unsafe fn alloc_l2() -> *mut PageTable {
    let idx = L2_NEXT;
    assert!(idx < 8, "out of L2 page tables");
    L2_NEXT += 1;
    &raw mut L2[idx]
}

#[derive(Clone, Copy)]
pub struct MemRegion {
    pub base: usize,
    pub size: usize,
    pub kind: MemKind,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MemKind {
    Ram,
    Device,
}

pub fn page_table_end() -> usize {
    // SAFETY: Address arithmetic on static tables.
    unsafe { &raw const L2[L2_NEXT] as usize }
}

pub unsafe fn init(regions: &[MemRegion]) {
    L2_NEXT = 0;

    // L0[0] → L1 table (covers first 512 GB).
    let l1_pa = &raw const L1 as u64;
    L0.entries[0] = l1_pa | PT_TABLE;

    for region in regions {
        if region.size == 0 {
            continue;
        }
        map_region(region);
    }

    // Configure MMU registers.
    core::arch::asm!("msr mair_el1, {}", in(reg) MAIR_VALUE);
    core::arch::asm!("msr tcr_el1, {}", in(reg) TCR_VALUE);

    let ttbr0 = &raw const L0 as u64;
    core::arch::asm!("msr ttbr0_el1, {}", in(reg) ttbr0);
    core::arch::asm!("isb");

    // Invalidate TLBs and caches before enabling.
    core::arch::asm!("tlbi vmalle1");
    core::arch::asm!("dsb sy");
    core::arch::asm!("isb");

    // Enable MMU, D-cache, I-cache via SCTLR_EL1.
    let mut sctlr: u64;
    core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    sctlr |= 1 << 0;  // M  — MMU enable
    sctlr |= 1 << 2;  // C  — data cache enable
    sctlr |= 1 << 12; // I  — instruction cache enable
    core::arch::asm!("msr sctlr_el1, {}", in(reg) sctlr);
    core::arch::asm!("isb");
}

unsafe fn map_region(region: &MemRegion) {
    let attrs = match region.kind {
        MemKind::Ram => NORMAL_RAM,
        MemKind::Device => DEVICE_MEM,
    };

    let mut addr = region.base & !(BLOCK_SIZE_2M - 1);
    let end = region.base + region.size;

    while addr < end {
        let l1_idx = (addr >> 30) & 0x1FF;
        let l2_idx = (addr >> 21) & 0x1FF;

        // Ensure L1 entry points to an L2 table.
        if L1.entries[l1_idx] == 0 {
            let l2_ptr = alloc_l2();
            L1.entries[l1_idx] = (l2_ptr as u64) | PT_TABLE;
        }

        // Find the L2 table from the L1 entry.
        let l2_pa = (L1.entries[l1_idx] & 0x0000_FFFF_FFFF_F000) as *mut PageTable;
        (*l2_pa).entries[l2_idx] = (addr as u64) | attrs;

        addr += BLOCK_SIZE_2M;
    }
}

pub fn enabled() -> bool {
    let sctlr: u64;
    unsafe { core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr) };
    sctlr & 1 != 0
}
