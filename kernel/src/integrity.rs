use crate::crypto::crc32;
use crate::kprintln;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

static BOOT_CRC: AtomicU32 = AtomicU32::new(0);
static LAST_CHECK_TICK: AtomicU64 = AtomicU64::new(0);
static CHECK_COUNT: AtomicU64 = AtomicU64::new(0);
static mut INITIALIZED: bool = false;
static mut TEXT_BASE: usize = 0;
static mut TEXT_SIZE: usize = 0;

pub fn init() {
    extern "C" {
        fn _start();
        static __data_start: u8;
    }

    let base = _start as *const () as usize;
    let end = &raw const __data_start as usize;
    let size = end - base;

    let text_slice = unsafe { core::slice::from_raw_parts(base as *const u8, size) };
    let crc = crc32::crc32(text_slice);

    BOOT_CRC.store(crc, Ordering::SeqCst);
    unsafe {
        TEXT_BASE = base;
        TEXT_SIZE = size;
        INITIALIZED = true;
    }

    kprintln!("integrity: .text CRC32={:#010x} ({} bytes)", crc, size);
}

pub fn verify() -> bool {
    if unsafe { !INITIALIZED } {
        return true;
    }

    let base = unsafe { TEXT_BASE };
    let size = unsafe { TEXT_SIZE };
    let text_slice = unsafe { core::slice::from_raw_parts(base as *const u8, size) };
    let current = crc32::crc32(text_slice);
    let expected = BOOT_CRC.load(Ordering::SeqCst);

    let tick = arch::aarch64::exceptions::tick_count();
    LAST_CHECK_TICK.store(tick, Ordering::Relaxed);
    CHECK_COUNT.fetch_add(1, Ordering::Relaxed);

    current == expected
}

pub fn boot_crc() -> u32 {
    BOOT_CRC.load(Ordering::SeqCst)
}

pub fn last_check_tick() -> u64 {
    LAST_CHECK_TICK.load(Ordering::Relaxed)
}

pub fn check_count() -> u64 {
    CHECK_COUNT.load(Ordering::Relaxed)
}

pub fn text_size() -> usize {
    unsafe { TEXT_SIZE }
}
