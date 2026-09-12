use crate::os_cfg;
use crate::spinlock::SpinLock;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};

const MAX_POOLS: usize = os_cfg::MAX_POOLS;
const FREE_END: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PoolErr {
    Ok,
    InvalidArg,
    NoMemory,
    InvalidBlock,
    RegistryFull,
}

pub struct OsPool {
    lock: SpinLock,
    base: *mut u8,
    blk_size: u32,
    blk_count: u32,
    free_head: AtomicU32,
    free_count: AtomicU32,
    initialized: bool,
}

// SAFETY: Pool access is serialized by the internal spinlock.
unsafe impl Sync for OsPool {}
unsafe impl Send for OsPool {}

impl OsPool {
    pub const fn uninit() -> Self {
        Self {
            lock: SpinLock::new(),
            base: core::ptr::null_mut(),
            blk_size: 0,
            blk_count: 0,
            free_head: AtomicU32::new(FREE_END),
            free_count: AtomicU32::new(0),
            initialized: false,
        }
    }
}

pub fn pool_create(pool: &mut OsPool, buf: *mut u8, blk_size: u32, blk_count: u32) -> PoolErr {
    if buf.is_null() || blk_size < 4 || blk_count == 0 {
        return PoolErr::InvalidArg;
    }
    // Block size must be at least 4 bytes (for the free-list link).
    // Align block size up to 8 bytes for pointer alignment.
    let aligned_blk = ((blk_size as usize + 7) & !7) as u32;

    let saved = pool.lock.lock();

    pool.base = buf;
    pool.blk_size = aligned_blk;
    pool.blk_count = blk_count;
    pool.initialized = true;

    // Thread the free list through each block's first 4 bytes.
    for i in 0..blk_count {
        let blk_ptr = unsafe { buf.add((i as usize) * (aligned_blk as usize)) };
        let next = if i + 1 < blk_count { i + 1 } else { FREE_END };
        // SAFETY: blk_ptr points within the caller-provided buffer.
        unsafe { core::ptr::write_volatile(blk_ptr as *mut u32, next) };
    }
    pool.free_head.store(0, Ordering::Relaxed);
    pool.free_count.store(blk_count, Ordering::Relaxed);

    pool.lock.unlock(saved);

    // Register in global pool registry for diagnostics.
    let _ = registry_add(pool as *const OsPool);

    PoolErr::Ok
}

pub fn pool_alloc(pool: &mut OsPool) -> Result<*mut u8, PoolErr> {
    let saved = pool.lock.lock();

    if !pool.initialized {
        pool.lock.unlock(saved);
        return Err(PoolErr::InvalidArg);
    }

    let head = pool.free_head.load(Ordering::Relaxed);
    if head == FREE_END {
        pool.lock.unlock(saved);
        return Err(PoolErr::NoMemory);
    }

    let blk_ptr = unsafe { pool.base.add((head as usize) * (pool.blk_size as usize)) };
    // SAFETY: blk_ptr is within the pool buffer; head is a valid index.
    let next = unsafe { core::ptr::read_volatile(blk_ptr as *const u32) };
    pool.free_head.store(next, Ordering::Relaxed);
    pool.free_count.fetch_sub(1, Ordering::Relaxed);

    pool.lock.unlock(saved);
    Ok(blk_ptr)
}

pub fn pool_free(pool: &mut OsPool, blk: *mut u8) -> PoolErr {
    if blk.is_null() {
        return PoolErr::InvalidBlock;
    }

    let saved = pool.lock.lock();

    if !pool.initialized {
        pool.lock.unlock(saved);
        return PoolErr::InvalidArg;
    }

    let offset = blk as usize - pool.base as usize;
    let blk_size = pool.blk_size as usize;

    // Validate: block must be within pool and aligned to block size.
    if !offset.is_multiple_of(blk_size) {
        pool.lock.unlock(saved);
        return PoolErr::InvalidBlock;
    }
    let idx = offset / blk_size;
    if idx >= pool.blk_count as usize {
        pool.lock.unlock(saved);
        return PoolErr::InvalidBlock;
    }

    // Push onto free list head.
    let old_head = pool.free_head.load(Ordering::Relaxed);
    // SAFETY: blk is a validated block pointer within the pool.
    unsafe { core::ptr::write_volatile(blk as *mut u32, old_head) };
    pool.free_head.store(idx as u32, Ordering::Relaxed);
    pool.free_count.fetch_add(1, Ordering::Relaxed);

    pool.lock.unlock(saved);
    PoolErr::Ok
}

pub fn pool_stats(pool: &OsPool) -> (u32, u32, u32) {
    (
        pool.blk_count,
        pool.free_count.load(Ordering::Relaxed),
        pool.blk_size,
    )
}

// --- Global pool registry (for diagnostics / shell) ---

struct PoolRegistry {
    pools: [*const OsPool; MAX_POOLS],
    count: usize,
}

// SAFETY: Pool pointers are valid for 'static lifetime (kernel statics).
unsafe impl Sync for PoolRegistry {}
unsafe impl Send for PoolRegistry {}

struct RegistryCell(UnsafeCell<PoolRegistry>);
unsafe impl Sync for RegistryCell {}

static POOL_LOCK: SpinLock = SpinLock::new();
static REGISTRY: RegistryCell = RegistryCell(UnsafeCell::new(PoolRegistry {
    pools: [core::ptr::null(); MAX_POOLS],
    count: 0,
}));

fn registry() -> &'static mut PoolRegistry {
    // SAFETY: Caller must hold POOL_LOCK.
    unsafe { &mut *REGISTRY.0.get() }
}

fn registry_add(pool: *const OsPool) -> PoolErr {
    let saved = POOL_LOCK.lock();
    let reg = registry();
    if reg.count >= MAX_POOLS {
        POOL_LOCK.unlock(saved);
        return PoolErr::RegistryFull;
    }
    reg.pools[reg.count] = pool;
    reg.count += 1;
    POOL_LOCK.unlock(saved);
    PoolErr::Ok
}

pub fn pool_count() -> usize {
    let saved = POOL_LOCK.lock();
    let c = registry().count;
    POOL_LOCK.unlock(saved);
    c
}

pub fn pool_info(idx: usize) -> Option<(u32, u32, u32)> {
    let saved = POOL_LOCK.lock();
    let reg = registry();
    if idx >= reg.count {
        POOL_LOCK.unlock(saved);
        return None;
    }
    let pool = unsafe { &*reg.pools[idx] };
    let stats = pool_stats(pool);
    POOL_LOCK.unlock(saved);
    Some(stats)
}
