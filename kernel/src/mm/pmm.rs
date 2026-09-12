// Bitmap-based physical page frame allocator.
//
// Each bit represents one 4KB page: 1 = used, 0 = free.
// Supports up to 4GB of physical memory (1048576 pages).

use arch::mm::PageAllocator;

const PAGE_SIZE: usize = 4096;
const MAX_PAGES: usize = 1 << 20; // 4 GB / 4 KB
const BITMAP_WORDS: usize = MAX_PAGES / 64;

pub struct BitmapAllocator {
    bitmap: [u64; BITMAP_WORDS],
    total: usize,
    used: usize,
}

impl BitmapAllocator {
    pub const fn new() -> Self {
        Self {
            bitmap: [0; BITMAP_WORDS],
            total: 0,
            used: 0,
        }
    }

    pub fn init(&mut self, _ram_base: usize, ram_size: usize) {
        self.total = ram_size / PAGE_SIZE;
        self.used = 0;
    }

    pub fn mark_range_used(&mut self, base: usize, size: usize) {
        let start_page = base / PAGE_SIZE;
        let end_page = (base + size).div_ceil(PAGE_SIZE);
        for page in start_page..end_page {
            if page < MAX_PAGES && !self.is_used(page) {
                self.set_used(page);
                self.used += 1;
            }
        }
    }

    fn is_used(&self, page: usize) -> bool {
        let word = page / 64;
        let bit = page % 64;
        self.bitmap[word] & (1u64 << bit) != 0
    }

    fn set_used(&mut self, page: usize) {
        let word = page / 64;
        let bit = page % 64;
        self.bitmap[word] |= 1u64 << bit;
    }

    fn set_free(&mut self, page: usize) {
        let word = page / 64;
        let bit = page % 64;
        self.bitmap[word] &= !(1u64 << bit);
    }
}

impl PageAllocator for BitmapAllocator {
    fn alloc_page(&mut self) -> Option<usize> {
        for word_idx in 0..BITMAP_WORDS {
            let word = self.bitmap[word_idx];
            if word != u64::MAX {
                let bit = (!word).trailing_zeros() as usize;
                let page = word_idx * 64 + bit;
                if page >= self.total {
                    return None;
                }
                self.bitmap[word_idx] |= 1u64 << bit;
                self.used += 1;
                return Some(page * PAGE_SIZE);
            }
        }
        None
    }

    fn alloc_pages(&mut self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        if count == 1 {
            return self.alloc_page();
        }
        let mut run_start = 0usize;
        let mut run_len = 0usize;
        for page in 0..self.total {
            if self.is_used(page) {
                run_start = page + 1;
                run_len = 0;
            } else {
                run_len += 1;
                if run_len == count {
                    for p in run_start..run_start + count {
                        self.set_used(p);
                    }
                    self.used += count;
                    return Some(run_start * PAGE_SIZE);
                }
            }
        }
        None
    }

    fn free_page(&mut self, pa: usize) {
        let page = pa / PAGE_SIZE;
        if page < self.total && self.is_used(page) {
            self.set_free(page);
            self.used -= 1;
        }
    }

    fn total_pages(&self) -> usize {
        self.total
    }

    fn used_pages(&self) -> usize {
        self.used
    }
}
