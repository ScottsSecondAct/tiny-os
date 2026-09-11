pub mod mbr;
pub mod cache;
pub mod ramdisk;

use arch::aarch64::emmc2;
use arch::block::{BlockDevice, BlockError};
use cache::BlockCache;
use core::cell::UnsafeCell;

struct Emmc2Wrapper;

impl BlockDevice for Emmc2Wrapper {
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        emmc2::read_block(lba, buf)
    }
    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        emmc2::write_block(lba, buf)
    }
    fn block_count(&self) -> u64 {
        emmc2::card_info().map(|(_, c)| c).unwrap_or(0)
    }
    fn block_size(&self) -> usize {
        512
    }
}

enum ActiveDevice {
    None,
    Emmc2,
    RamDisk,
}

struct StorageState {
    cache: BlockCache,
    active: ActiveDevice,
}

struct StorageCell(UnsafeCell<StorageState>);
// SAFETY: Single-task access enforced by caller (shell task only).
unsafe impl Sync for StorageCell {}

static STORAGE: StorageCell = StorageCell(UnsafeCell::new(StorageState {
    cache: BlockCache::new(),
    active: ActiveDevice::None,
}));

struct EmccCell(UnsafeCell<Emmc2Wrapper>);
// SAFETY: Single-task access enforced by caller.
unsafe impl Sync for EmccCell {}

static EMMC_DEV: EmccCell = EmccCell(UnsafeCell::new(Emmc2Wrapper));

fn emmc2_dev() -> &'static mut dyn BlockDevice {
    // SAFETY: Single-task access.
    unsafe { &mut *EMMC_DEV.0.get() }
}

fn state() -> &'static mut StorageState {
    // SAFETY: Single-task access — only shell/boot code calls storage functions.
    unsafe { &mut *STORAGE.0.get() }
}

fn dev() -> &'static mut dyn BlockDevice {
    match state().active {
        ActiveDevice::Emmc2 => emmc2_dev(),
        ActiveDevice::RamDisk => ramdisk::device(),
        ActiveDevice::None => panic!("storage: no device"),
    }
}

pub fn init() {
    if emmc2::is_initialized() {
        state().active = ActiveDevice::Emmc2;
    } else {
        ramdisk::init();
        state().active = ActiveDevice::RamDisk;
    }
}

pub fn is_initialized() -> bool {
    !matches!(state().active, ActiveDevice::None)
}

pub fn cached_read(lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
    let s = state();
    let d = match s.active {
        ActiveDevice::Emmc2 => {
            static mut EMMC: Emmc2Wrapper = Emmc2Wrapper;
            // SAFETY: Single-task access.
            unsafe { &mut EMMC as &mut dyn BlockDevice }
        }
        ActiveDevice::RamDisk => ramdisk::device(),
        ActiveDevice::None => return Err(BlockError::NoMedia),
    };
    s.cache.read(lba, buf, d)
}

pub fn cached_write(lba: u64, buf: &[u8]) -> Result<(), BlockError> {
    let s = state();
    let d = match s.active {
        ActiveDevice::Emmc2 => {
            static mut EMMC: Emmc2Wrapper = Emmc2Wrapper;
            // SAFETY: Single-task access.
            unsafe { &mut EMMC as &mut dyn BlockDevice }
        }
        ActiveDevice::RamDisk => ramdisk::device(),
        ActiveDevice::None => return Err(BlockError::NoMedia),
    };
    s.cache.write(lba, buf, d)
}

pub fn flush() -> Result<(), BlockError> {
    let s = state();
    let d = match s.active {
        ActiveDevice::Emmc2 => {
            static mut EMMC: Emmc2Wrapper = Emmc2Wrapper;
            // SAFETY: Single-task access.
            unsafe { &mut EMMC as &mut dyn BlockDevice }
        }
        ActiveDevice::RamDisk => ramdisk::device(),
        ActiveDevice::None => return Err(BlockError::NoMedia),
    };
    s.cache.flush(d)
}

pub fn cache_stats() -> (u64, u64) {
    state().cache.stats()
}

pub fn find_fat32_partition() -> Option<(u32, u32)> {
    let d = dev();
    if let Ok(parts) = mbr::parse_mbr(d) {
        for p in parts.iter().flatten() {
            if p.part_type == 0x0B || p.part_type == 0x0C {
                return Some((p.lba_start, p.sector_count));
            }
        }
    }
    None
}

pub fn print_mbr_info() {
    let d = dev();
    match mbr::parse_mbr(d) {
        Ok(parts) => {
            let mut found = false;
            for (i, p) in parts.iter().enumerate() {
                if let Some(part) = p {
                    found = true;
                    crate::kprintln!(
                        "  partition {}: {} {} LBA {}..{} ({} MB)",
                        i,
                        part.type_name(),
                        if part.is_active() { "*" } else { " " },
                        part.lba_start,
                        part.lba_start + part.sector_count,
                        part.size_mb()
                    );
                }
            }
            if !found {
                crate::kprintln!("  no partitions found");
            }
        }
        Err(_) => {
            crate::kprintln!("  failed to read MBR");
        }
    }
}
