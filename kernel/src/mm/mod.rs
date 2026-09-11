pub mod dtb;
pub mod heap;
pub mod pmm;

use core::cell::UnsafeCell;
use arch::aarch64::mmu::{self, MemKind, MemRegion};
use arch::mm::PageAllocator;
use crate::kprintln;

const PAGE_SIZE: usize = 4096;
const HEAP_PAGES: usize = 64; // 256 KB initial heap

extern "C" {
    static __stack_top: u8;
    static __data_start: u8;
}

struct PmmCell(UnsafeCell<pmm::BitmapAllocator>);
unsafe impl Sync for PmmCell {}

static PMM: PmmCell = PmmCell(UnsafeCell::new(pmm::BitmapAllocator::new()));

fn pmm() -> &'static mut pmm::BitmapAllocator {
    // SAFETY: Single-core, no ISR access to PMM.
    unsafe { &mut *PMM.0.get() }
}

pub fn init() {
    let (ram_base, ram_size) = match dtb::find_memory() {
        Some(r) => (r.base as usize, r.size as usize),
        None => bsp_default_ram(),
    };

    kprintln!("RAM: {:#x} - {:#x} ({} MB)", ram_base, ram_base + ram_size, ram_size >> 20);

    pmm().init(ram_base, ram_size);

    // Reserve everything from 0 to kernel end (code + BSS + stack).
    let kernel_end = unsafe { &__stack_top as *const u8 as usize };
    pmm().mark_range_used(0, kernel_end);
    kprintln!("kernel: {:#x} - {:#x} ({} KB reserved)", 0x80000, kernel_end, (kernel_end - 0x80000) >> 10);

    // Build MMU identity-mapping regions and enable.
    let regions = bsp_mem_regions(ram_base, ram_size);
    let data_start = unsafe { &__data_start as *const u8 as usize };
    unsafe { mmu::init(&regions) };
    kprintln!("MMU: enabled, identity-mapped, W^X, caches on");
    kprintln!("W^X: code RX {:#x}-{:#x}, data RW+NX {:#x}+", 0x80000usize, data_start, data_start);

    // Seed the heap from PMM pages.
    let mut heap_bytes = 0usize;
    for _ in 0..HEAP_PAGES {
        if let Some(pa) = pmm().alloc_page() {
            unsafe { heap::add_region(pa, PAGE_SIZE) };
            heap_bytes += PAGE_SIZE;
        }
    }
    kprintln!("heap: {} KB", heap_bytes >> 10);
}

pub fn page_stats() -> (usize, usize, usize) {
    let p = pmm();
    (p.total_pages(), p.used_pages(), p.free_pages())
}

pub fn heap_stats() -> (usize, usize, usize) {
    heap::stats()
}

fn bsp_default_ram() -> (usize, usize) {
    #[cfg(feature = "bsp-qemu")]
    {
        (bsp::qemu_virt::memory_map::RAM_BASE, bsp::qemu_virt::memory_map::RAM_SIZE_DEFAULT)
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        (bsp::rpi5::memory_map::RAM_BASE, bsp::rpi5::memory_map::RAM_SIZE_DEFAULT)
    }
}

fn bsp_mem_regions(ram_base: usize, ram_size: usize) -> [MemRegion; 6] {
    // W^X: split kernel into code (RX, no write) and data (RW, no execute).
    // __data_start is 2MB-aligned by the linker script. The first 2MB block
    // contains both firmware data (0-0x80000) and kernel code; mapped RX
    // since we don't need to write to either from the kernel.
    let data_start = unsafe { &__data_start as *const u8 as usize };
    let code_size = data_start - ram_base;
    let data_size = ram_base + ram_size - data_start;

    #[cfg(feature = "bsp-qemu")]
    {
        use bsp::qemu_virt::memory_map as mm;
        [
            MemRegion { base: ram_base, size: code_size, kind: MemKind::RoCode },
            MemRegion { base: data_start, size: data_size, kind: MemKind::Ram },
            MemRegion { base: mm::PERIPH_BASE, size: mm::PERIPH_SIZE, kind: MemKind::Device },
            MemRegion { base: 0, size: 0, kind: MemKind::Ram },
            MemRegion { base: 0, size: 0, kind: MemKind::Ram },
            MemRegion { base: 0, size: 0, kind: MemKind::Ram },
        ]
    }
    #[cfg(feature = "bsp-rpi5")]
    {
        use bsp::rpi5::memory_map as mm;
        [
            MemRegion { base: ram_base, size: code_size, kind: MemKind::RoCode },
            MemRegion { base: data_start, size: data_size, kind: MemKind::Ram },
            MemRegion { base: mm::PERIPH_BASE, size: mm::PERIPH_SIZE, kind: MemKind::Device },
            MemRegion { base: mm::RP1_PERIPH_BASE, size: mm::RP1_PERIPH_SIZE, kind: MemKind::Device },
            MemRegion { base: 0, size: 0, kind: MemKind::Ram },
            MemRegion { base: 0, size: 0, kind: MemKind::Ram },
        ]
    }
}
