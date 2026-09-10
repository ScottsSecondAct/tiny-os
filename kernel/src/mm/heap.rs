// Simple linked-list heap allocator for kernel use.
//
// Backed by page frames from the PMM. Provides kmalloc/kfree for
// variable-size kernel allocations. Not used in ISR context.

use core::cell::UnsafeCell;

const MIN_BLOCK_SIZE: usize = 32;

#[repr(C)]
struct FreeBlock {
    size: usize,
    next: Option<*mut FreeBlock>,
}

pub struct HeapAllocator {
    head: Option<*mut FreeBlock>,
    total_bytes: usize,
    used_bytes: usize,
}

// SAFETY: Single-core, no ISR access.
unsafe impl Send for HeapAllocator {}

impl HeapAllocator {
    pub const fn new() -> Self {
        Self {
            head: None,
            total_bytes: 0,
            used_bytes: 0,
        }
    }

    pub unsafe fn add_region(&mut self, base: usize, size: usize) {
        if size < MIN_BLOCK_SIZE {
            return;
        }
        let block = base as *mut FreeBlock;
        (*block).size = size;
        (*block).next = self.head;
        self.head = Some(block);
        self.total_bytes += size;
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let alloc_size = align_up(size + core::mem::size_of::<usize>(), align.max(8));

        let mut prev: Option<*mut FreeBlock> = None;
        let mut current = self.head;

        while let Some(block) = current {
            let block_ref = unsafe { &mut *block };
            if block_ref.size >= alloc_size {
                let remainder = block_ref.size - alloc_size;
                if remainder >= MIN_BLOCK_SIZE {
                    // Split: create a new free block after the allocation.
                    let new_block = unsafe { (block as *mut u8).add(alloc_size) as *mut FreeBlock };
                    unsafe {
                        (*new_block).size = remainder;
                        (*new_block).next = block_ref.next;
                    }
                    // Remove original, insert remainder.
                    match prev {
                        Some(p) => unsafe { (*p).next = Some(new_block) },
                        None => self.head = Some(new_block),
                    }
                } else {
                    // Use the whole block.
                    match prev {
                        Some(p) => unsafe { (*p).next = block_ref.next },
                        None => self.head = block_ref.next,
                    }
                }

                // Store the allocation size in a header word.
                unsafe {
                    let header = block as *mut usize;
                    *header = alloc_size;
                }
                self.used_bytes += alloc_size;
                let data = unsafe { (block as *mut u8).add(core::mem::size_of::<usize>()) };
                return Some(data);
            }

            prev = current;
            current = block_ref.next;
        }

        None
    }

    pub fn free(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        unsafe {
            let header = ptr.sub(core::mem::size_of::<usize>()) as *mut usize;
            let alloc_size = *header;
            self.used_bytes -= alloc_size;

            let block = header as *mut FreeBlock;
            (*block).size = alloc_size;
            (*block).next = self.head;
            self.head = Some(block);
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn free_bytes(&self) -> usize {
        self.total_bytes - self.used_bytes
    }
}

fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

// Global heap instance.
struct HeapCell(UnsafeCell<HeapAllocator>);
unsafe impl Sync for HeapCell {}

static HEAP: HeapCell = HeapCell(UnsafeCell::new(HeapAllocator::new()));

pub unsafe fn add_region(base: usize, size: usize) {
    (*HEAP.0.get()).add_region(base, size);
}

pub fn kmalloc(size: usize, align: usize) -> Option<*mut u8> {
    // SAFETY: Single-core, no ISR access to heap.
    unsafe { (*HEAP.0.get()).alloc(size, align) }
}

pub fn kfree(ptr: *mut u8) {
    unsafe { (*HEAP.0.get()).free(ptr) }
}

pub fn stats() -> (usize, usize, usize) {
    unsafe {
        let h = &*HEAP.0.get();
        (h.total_bytes(), h.used_bytes(), h.free_bytes())
    }
}
