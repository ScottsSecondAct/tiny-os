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
const AP_RO_EL1: u64 = 2 << 6;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

const fn attr_idx(idx: u64) -> u64 {
    (idx & 7) << 2
}

// Block descriptor templates.
// W^X: code is read-only + executable; data is read-write + no-execute.
const NORMAL_CODE: u64 = PT_BLOCK | attr_idx(1) | AF | SH_INNER | AP_RO_EL1 | UXN;
const NORMAL_RAM: u64 = PT_BLOCK | attr_idx(1) | AF | SH_INNER | AP_RW_EL1 | PXN | UXN;
const NORMAL_NC: u64 = PT_BLOCK | attr_idx(2) | AF | SH_INNER | AP_RW_EL1 | PXN | UXN;
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
    RoCode,
    Ram,
    NonCacheable,
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
        MemKind::RoCode => NORMAL_CODE,
        MemKind::Ram => NORMAL_RAM,
        MemKind::NonCacheable => NORMAL_NC,
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

/// Enable the MMU on a secondary core using the page tables already
/// built by the primary core.
pub unsafe fn init_secondary() {
    core::arch::asm!("msr mair_el1, {}", in(reg) MAIR_VALUE);
    core::arch::asm!("msr tcr_el1, {}", in(reg) TCR_VALUE);

    let ttbr0 = &raw const L0 as u64;
    core::arch::asm!("msr ttbr0_el1, {}", in(reg) ttbr0);
    core::arch::asm!("isb");

    core::arch::asm!("tlbi vmalle1");
    core::arch::asm!("dsb sy");
    core::arch::asm!("isb");

    let mut sctlr: u64;
    core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr);
    sctlr |= 1 << 0;  // M
    sctlr |= 1 << 2;  // C
    sctlr |= 1 << 12; // I
    core::arch::asm!("msr sctlr_el1, {}", in(reg) sctlr);
    core::arch::asm!("isb");
}

pub fn enabled() -> bool {
    let sctlr: u64;
    unsafe { core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr) };
    sctlr & 1 != 0
}

pub fn kernel_ttbr0() -> u64 {
    unsafe { &raw const L0 as u64 }
}

// --- User-mode page table support ---

const PT_PAGE: u64 = 0b11;
const NG: u64 = 1 << 11;
const AP_RW_EL0: u64 = 1 << 6;
const AP_RO_EL0: u64 = 3 << 6;
pub const PAGE_SIZE_4K: usize = 4096;

const MAX_USER_TASKS: usize = 4;
const USER_L2_POOL_SIZE: usize = 8;
const L3_POOL_SIZE: usize = 16;
const BLOCK_ATTR_MASK: u64 = 0x0070_0000_0000_0FFC;

static mut USER_L0: [PageTable; MAX_USER_TASKS] = [
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
];
static mut USER_L1: [PageTable; MAX_USER_TASKS] = [
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
];
static mut USER_L2_POOL: [PageTable; USER_L2_POOL_SIZE] = [
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
];
static mut USER_L2_NEXT: usize = 0;
static mut L3_POOL: [PageTable; L3_POOL_SIZE] = [
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
    PageTable::zero(), PageTable::zero(), PageTable::zero(), PageTable::zero(),
];
static mut L3_NEXT: usize = 0;
static mut USER_SLOT_USED: [bool; MAX_USER_TASKS] = [false; MAX_USER_TASKS];
static mut NEXT_ASID: u8 = 1;

unsafe fn alloc_user_l2() -> *mut PageTable {
    let idx = USER_L2_NEXT;
    assert!(idx < USER_L2_POOL_SIZE, "out of user L2 tables");
    USER_L2_NEXT += 1;
    &raw mut USER_L2_POOL[idx]
}

unsafe fn alloc_l3() -> *mut PageTable {
    let idx = L3_NEXT;
    assert!(idx < L3_POOL_SIZE, "out of L3 tables");
    L3_NEXT += 1;
    &raw mut L3_POOL[idx]
}

fn alloc_asid() -> u8 {
    unsafe {
        let asid = NEXT_ASID;
        NEXT_ASID = NEXT_ASID.wrapping_add(1);
        if NEXT_ASID == 0 { NEXT_ASID = 1; }
        asid
    }
}

/// Create a per-task page table for an EL0 user task.
///
/// Clones kernel page tables, marks user_code region as EL0-executable (RO),
/// maps user_stack region as EL0-writable with a 4KB guard page below.
///
/// Returns TTBR0 value with embedded ASID.
pub unsafe fn create_user_page_table(
    user_code_base: usize,
    user_code_size: usize,
    user_stack_base: usize,
    user_stack_pages: usize,
) -> u64 {
    let slot = USER_SLOT_USED.iter().position(|&used| !used)
        .expect("no free user page table slots");
    USER_SLOT_USED[slot] = true;
    let asid = alloc_asid();

    for e in USER_L0[slot].entries.iter_mut() { *e = 0; }
    for e in USER_L1[slot].entries.iter_mut() { *e = 0; }

    let user_l1_pa = &raw const USER_L1[slot] as u64;
    USER_L0[slot].entries[0] = user_l1_pa | PT_TABLE;

    let code_end = user_code_base + user_code_size;
    let stack_end = user_stack_base + user_stack_pages * PAGE_SIZE_4K;

    for i in 0..ENTRIES_PER_TABLE {
        if L1.entries[i] == 0 { continue; }

        let kernel_l2_pa = L1.entries[i] & 0x0000_FFFF_FFFF_F000;
        let gb_base = i << 30;
        let gb_end = gb_base + (1 << 30);

        let code_overlaps = user_code_size > 0
            && user_code_base < gb_end && code_end > gb_base;
        let stack_overlaps = user_stack_pages > 0
            && user_stack_base < gb_end && stack_end > gb_base;

        if code_overlaps || stack_overlaps {
            let user_l2 = alloc_user_l2();
            let kernel_l2 = kernel_l2_pa as *const PageTable;
            core::ptr::copy_nonoverlapping(
                (*kernel_l2).entries.as_ptr(),
                (*user_l2).entries.as_mut_ptr(),
                ENTRIES_PER_TABLE,
            );

            if code_overlaps {
                let mut addr = user_code_base & !(BLOCK_SIZE_2M - 1);
                while addr < code_end {
                    let l2_idx = (addr >> 21) & 0x1FF;
                    let entry = &mut (*user_l2).entries[l2_idx];
                    if *entry != 0 {
                        *entry &= !(3u64 << 6);
                        *entry |= AP_RW_EL0;
                        *entry &= !UXN;
                        *entry |= PXN | NG;
                    }
                    addr += BLOCK_SIZE_2M;
                }
            }

            if stack_overlaps {
                let block_base = user_stack_base & !(BLOCK_SIZE_2M - 1);
                let l2_idx = (block_base >> 21) & 0x1FF;
                let old_entry = (*user_l2).entries[l2_idx];
                let page_attrs = (old_entry & BLOCK_ATTR_MASK) | PT_PAGE;

                let l3 = alloc_l3();
                for j in 0..ENTRIES_PER_TABLE {
                    let page_pa = (block_base + j * PAGE_SIZE_4K) as u64;
                    (*l3).entries[j] = page_pa | page_attrs;
                }

                let guard_page = user_stack_base.wrapping_sub(PAGE_SIZE_4K);
                if guard_page >= block_base {
                    let guard_idx = (guard_page - block_base) / PAGE_SIZE_4K;
                    (*l3).entries[guard_idx] = 0;
                }

                for p in 0..user_stack_pages {
                    let page_addr = user_stack_base + p * PAGE_SIZE_4K;
                    let l3_idx = (page_addr - block_base) / PAGE_SIZE_4K;
                    let pa = page_addr as u64;
                    (*l3).entries[l3_idx] = pa | PT_PAGE | attr_idx(1) | AF
                        | SH_INNER | AP_RW_EL0 | PXN | UXN | NG;
                }

                (*user_l2).entries[l2_idx] = (l3 as u64) | PT_TABLE;
            }

            USER_L1[slot].entries[i] = (user_l2 as u64) | PT_TABLE;
        } else {
            USER_L1[slot].entries[i] = L1.entries[i];
        }
    }

    let l0_pa = &raw const USER_L0[slot] as u64;
    l0_pa | ((asid as u64) << 48)
}

pub struct UserMapping {
    pub base: usize,
    pub pages: usize,
    pub executable: bool,
}

/// Create a per-task page table for dynamically loaded EL0 binaries.
///
/// Each UserMapping describes a contiguous set of identity-mapped 4KB pages
/// with EL0 permissions: executable=true → RX, executable=false → RW.
/// A guard page is placed below the last mapping (assumed to be the stack).
pub unsafe fn create_user_page_table_mapped(mappings: &[UserMapping]) -> u64 {
    let slot = USER_SLOT_USED.iter().position(|&used| !used)
        .expect("no free user page table slots");
    USER_SLOT_USED[slot] = true;
    let asid = alloc_asid();

    for e in USER_L0[slot].entries.iter_mut() { *e = 0; }
    for e in USER_L1[slot].entries.iter_mut() { *e = 0; }

    let user_l1_pa = &raw const USER_L1[slot] as u64;
    USER_L0[slot].entries[0] = user_l1_pa | PT_TABLE;

    // Copy kernel L1 entries as baseline.
    for i in 0..ENTRIES_PER_TABLE {
        USER_L1[slot].entries[i] = L1.entries[i];
    }

    // For each mapping, create L3 page table entries with EL0 permissions.
    for (mi, mapping) in mappings.iter().enumerate() {
        if mapping.pages == 0 { continue; }
        let region_end = mapping.base + mapping.pages * PAGE_SIZE_4K;

        // Process each 2MB block that this mapping touches.
        let mut block_base = mapping.base & !(BLOCK_SIZE_2M - 1);
        while block_base < region_end {
            let l1_idx = (block_base >> 30) & 0x1FF;
            let l2_idx = (block_base >> 21) & 0x1FF;

            // Ensure we have a private L2 table for this GB range.
            let l2_entry = USER_L1[slot].entries[l1_idx];
            let user_l2 = if l2_entry & 0b11 == PT_TABLE {
                let l2_pa = l2_entry & 0x0000_FFFF_FFFF_F000;
                // Check if this is still a kernel L2 — if so, clone it.
                let is_kernel_l2 = L1.entries[l1_idx] == l2_entry;
                if is_kernel_l2 {
                    let new_l2 = alloc_user_l2();
                    let kernel_l2 = l2_pa as *const PageTable;
                    core::ptr::copy_nonoverlapping(
                        (*kernel_l2).entries.as_ptr(),
                        (*new_l2).entries.as_mut_ptr(),
                        ENTRIES_PER_TABLE,
                    );
                    USER_L1[slot].entries[l1_idx] = (new_l2 as u64) | PT_TABLE;
                    new_l2
                } else {
                    l2_pa as *mut PageTable
                }
            } else {
                continue;
            };

            // Get or create L3 table for this 2MB block.
            let l2_val = (*user_l2).entries[l2_idx];
            let l3 = if l2_val & 0b11 == PT_TABLE {
                // Already an L3 table (from a previous mapping in this block).
                (l2_val & 0x0000_FFFF_FFFF_F000) as *mut PageTable
            } else {
                // Replace 2MB block with L3 table, copying block attributes.
                let new_l3 = alloc_l3();
                let page_attrs = (l2_val & BLOCK_ATTR_MASK) | PT_PAGE;
                for j in 0..ENTRIES_PER_TABLE {
                    let page_pa = (block_base + j * PAGE_SIZE_4K) as u64;
                    (*new_l3).entries[j] = page_pa | page_attrs;
                }
                (*user_l2).entries[l2_idx] = (new_l3 as u64) | PT_TABLE;
                new_l3
            };

            // Set EL0 permissions on pages within this mapping.
            let overlap_start = mapping.base.max(block_base);
            let overlap_end = region_end.min(block_base + BLOCK_SIZE_2M);
            for addr in (overlap_start..overlap_end).step_by(PAGE_SIZE_4K) {
                let l3_idx = (addr - block_base) / PAGE_SIZE_4K;
                let pa = addr as u64;
                if mapping.executable {
                    // EL0 RX: read-only, executable, non-global.
                    (*l3).entries[l3_idx] = pa | PT_PAGE | attr_idx(1) | AF
                        | SH_INNER | AP_RO_EL0 | PXN | NG;
                } else {
                    // EL0 RW: read-write, no-execute, non-global.
                    (*l3).entries[l3_idx] = pa | PT_PAGE | attr_idx(1) | AF
                        | SH_INNER | AP_RW_EL0 | PXN | UXN | NG;
                }
            }

            // Place a guard page below the last mapping (stack).
            if mi == mappings.len() - 1 {
                let guard_page = mapping.base.wrapping_sub(PAGE_SIZE_4K);
                if guard_page >= block_base && guard_page < block_base + BLOCK_SIZE_2M {
                    let guard_idx = (guard_page - block_base) / PAGE_SIZE_4K;
                    (*l3).entries[guard_idx] = 0;
                }
            }

            block_base += BLOCK_SIZE_2M;
        }
    }

    let l0_pa = &raw const USER_L0[slot] as u64;
    l0_pa | ((asid as u64) << 48)
}

pub unsafe fn free_user_page_table(ttbr0: u64) {
    let l0_pa = ttbr0 & 0x0000_FFFF_FFFF_F000;
    for slot in 0..MAX_USER_TASKS {
        if &raw const USER_L0[slot] as u64 == l0_pa {
            USER_SLOT_USED[slot] = false;
            return;
        }
    }
}

pub unsafe fn switch_ttbr0(ttbr0: u64) {
    core::arch::asm!(
        "msr ttbr0_el1, {ttbr}",
        "isb",
        "tlbi vmalle1is",
        "dsb ish",
        "isb",
        ttbr = in(reg) ttbr0,
    );
}
