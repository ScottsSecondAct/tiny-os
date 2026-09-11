use core::sync::atomic::{AtomicU8, Ordering};
use crate::spinlock::SpinLock;
use crate::mm::{DMA_POOL_BASE, DMA_POOL_SIZE};

const BUF_SLOT_SIZE: usize = 2048;
const BUF_DATA_CAPACITY: u16 = 1536;
const BUF_HEADROOM: u16 = 64;
const MAX_BUFS: usize = DMA_POOL_SIZE / BUF_SLOT_SIZE;

pub struct NetBuf {
    pub data: *mut u8,
    pub head: u16,
    pub tail: u16,
    pub capacity: u16,
    pub refcount: AtomicU8,
}

impl NetBuf {
    pub fn len(&self) -> usize {
        (self.tail - self.head) as usize
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: data is a valid NC buffer from the DMA pool.
        unsafe { core::slice::from_raw_parts(self.data.add(self.head as usize), self.len()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: data is a valid NC buffer from the DMA pool.
        unsafe {
            core::slice::from_raw_parts_mut(self.data.add(self.head as usize), self.len())
        }
    }

    pub fn push_data(&mut self, src: &[u8]) -> bool {
        let avail = (self.capacity - self.tail) as usize;
        if src.len() > avail {
            return false;
        }
        // SAFETY: bounds checked above, data points to valid NC memory.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.as_ptr(),
                self.data.add(self.tail as usize),
                src.len(),
            );
        }
        self.tail += src.len() as u16;
        true
    }

    pub fn prepend_header(&mut self, hdr: &[u8]) -> bool {
        if (self.head as usize) < hdr.len() {
            return false;
        }
        self.head -= hdr.len() as u16;
        // SAFETY: head was decremented only if there was enough headroom.
        unsafe {
            core::ptr::copy_nonoverlapping(
                hdr.as_ptr(),
                self.data.add(self.head as usize),
                hdr.len(),
            );
        }
        true
    }

    pub fn reset(&mut self) {
        self.head = BUF_HEADROOM;
        self.tail = BUF_HEADROOM;
    }

    pub fn dma_addr(&self) -> usize {
        self.data as usize + self.head as usize
    }
}

struct Pool {
    bufs: [NetBuf; MAX_BUFS],
    free_head: u16,
    free_count: u16,
    total: u16,
    initialized: bool,
}

static POOL_LOCK: SpinLock = SpinLock::new();

struct PoolCell(core::cell::UnsafeCell<Pool>);
// SAFETY: Access protected by POOL_LOCK.
unsafe impl Sync for PoolCell {}

const UNINIT_NETBUF: NetBuf = NetBuf {
    data: core::ptr::null_mut(),
    head: 0,
    tail: 0,
    capacity: 0,
    refcount: AtomicU8::new(0),
};

static POOL: PoolCell = PoolCell(core::cell::UnsafeCell::new(Pool {
    bufs: [UNINIT_NETBUF; MAX_BUFS],
    free_head: 0xFFFF,
    free_count: 0,
    total: 0,
    initialized: false,
}));

fn pool() -> &'static mut Pool {
    // SAFETY: Caller must hold POOL_LOCK.
    unsafe { &mut *POOL.0.get() }
}

pub fn init() {
    let saved = POOL_LOCK.lock();
    let p = pool();

    let num_bufs = DMA_POOL_SIZE / BUF_SLOT_SIZE;
    p.total = num_bufs as u16;
    p.free_count = num_bufs as u16;

    for i in 0..num_bufs {
        let slot_addr = DMA_POOL_BASE + i * BUF_SLOT_SIZE;
        p.bufs[i].data = slot_addr as *mut u8;
        p.bufs[i].head = BUF_HEADROOM;
        p.bufs[i].tail = BUF_HEADROOM;
        p.bufs[i].capacity = BUF_DATA_CAPACITY;
        p.bufs[i].refcount = AtomicU8::new(0);
        // Thread free list through refcount=0 buffers using a chain index
        // stored as the next 2 bytes after the slot header area.
        // We use a simple index chain: bufs[i] -> bufs[i+1] -> ... -> 0xFFFF
        if i + 1 < num_bufs {
            // SAFETY: Writing the free-list next index into the NC buffer slot.
            unsafe {
                core::ptr::write_volatile(slot_addr as *mut u16, (i + 1) as u16);
            }
        } else {
            unsafe {
                core::ptr::write_volatile(slot_addr as *mut u16, 0xFFFFu16);
            }
        }
    }
    p.free_head = 0;
    p.initialized = true;

    POOL_LOCK.unlock(saved);
}

pub fn alloc() -> Option<&'static mut NetBuf> {
    let saved = POOL_LOCK.lock();
    let p = pool();

    if !p.initialized || p.free_head == 0xFFFF {
        POOL_LOCK.unlock(saved);
        return None;
    }

    let idx = p.free_head as usize;
    let buf = &mut p.bufs[idx];

    // Read next free index from the NC buffer slot.
    let next = unsafe { core::ptr::read_volatile(buf.data as *const u16) };
    p.free_head = next;
    p.free_count -= 1;

    buf.head = BUF_HEADROOM;
    buf.tail = BUF_HEADROOM;
    buf.refcount.store(1, Ordering::Relaxed);

    // SAFETY: The buffer is exclusively owned (refcount=1), and the pool
    // struct has 'static lifetime.
    let buf_ref = unsafe { &mut *(buf as *mut NetBuf) };

    POOL_LOCK.unlock(saved);
    Some(buf_ref)
}

pub fn free(buf: &mut NetBuf) {
    let old = buf.refcount.fetch_sub(1, Ordering::Release);
    if old != 1 {
        return;
    }

    let saved = POOL_LOCK.lock();
    let p = pool();

    let buf_addr = buf.data as usize;
    let idx = (buf_addr - DMA_POOL_BASE) / BUF_SLOT_SIZE;

    // Write current free_head as next pointer into the NC slot.
    unsafe { core::ptr::write_volatile(buf.data as *mut u16, p.free_head) };
    p.free_head = idx as u16;
    p.free_count += 1;

    POOL_LOCK.unlock(saved);
}

pub fn pool_stats() -> (u16, u16) {
    let saved = POOL_LOCK.lock();
    let p = pool();
    let stats = (p.total, p.free_count);
    POOL_LOCK.unlock(saved);
    stats
}
