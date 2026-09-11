use arch::block::{BlockDevice, BlockError};
use crate::os_cfg;

const CACHE_LINES: usize = os_cfg::CACHE_LINES;
const BLOCK_SIZE: usize = 512;

struct CacheLine {
    lba: u64,
    valid: bool,
    dirty: bool,
    last_used: u64,
    data: [u8; BLOCK_SIZE],
}

impl CacheLine {
    const fn empty() -> Self {
        Self {
            lba: 0,
            valid: false,
            dirty: false,
            last_used: 0,
            data: [0; BLOCK_SIZE],
        }
    }
}

pub struct BlockCache {
    lines: [CacheLine; CACHE_LINES],
    access_counter: u64,
    hits: u64,
    misses: u64,
}

impl BlockCache {
    pub const fn new() -> Self {
        const EMPTY: CacheLine = CacheLine::empty();
        Self {
            lines: [EMPTY; CACHE_LINES],
            access_counter: 0,
            hits: 0,
            misses: 0,
        }
    }

    fn find_line(&self, lba: u64) -> Option<usize> {
        for i in 0..CACHE_LINES {
            if self.lines[i].valid && self.lines[i].lba == lba {
                return Some(i);
            }
        }
        None
    }

    fn find_lru(&self) -> usize {
        let mut lru_idx = 0;
        let mut lru_time = u64::MAX;
        for i in 0..CACHE_LINES {
            if !self.lines[i].valid {
                return i;
            }
            if self.lines[i].last_used < lru_time {
                lru_time = self.lines[i].last_used;
                lru_idx = i;
            }
        }
        lru_idx
    }

    fn evict(&mut self, idx: usize, dev: &mut dyn BlockDevice) -> Result<(), BlockError> {
        if self.lines[idx].valid && self.lines[idx].dirty {
            dev.write_block(self.lines[idx].lba, &self.lines[idx].data)?;
            self.lines[idx].dirty = false;
        }
        self.lines[idx].valid = false;
        Ok(())
    }

    pub fn read(
        &mut self,
        lba: u64,
        buf: &mut [u8],
        dev: &mut dyn BlockDevice,
    ) -> Result<(), BlockError> {
        self.access_counter += 1;

        if let Some(idx) = self.find_line(lba) {
            self.hits += 1;
            self.lines[idx].last_used = self.access_counter;
            buf[..BLOCK_SIZE].copy_from_slice(&self.lines[idx].data);
            return Ok(());
        }

        self.misses += 1;
        let idx = self.find_lru();
        self.evict(idx, dev)?;

        dev.read_block(lba, &mut self.lines[idx].data)?;
        self.lines[idx].lba = lba;
        self.lines[idx].valid = true;
        self.lines[idx].dirty = false;
        self.lines[idx].last_used = self.access_counter;

        buf[..BLOCK_SIZE].copy_from_slice(&self.lines[idx].data);
        Ok(())
    }

    pub fn write(
        &mut self,
        lba: u64,
        buf: &[u8],
        dev: &mut dyn BlockDevice,
    ) -> Result<(), BlockError> {
        self.access_counter += 1;

        if let Some(idx) = self.find_line(lba) {
            self.hits += 1;
            self.lines[idx].data[..BLOCK_SIZE].copy_from_slice(&buf[..BLOCK_SIZE]);
            self.lines[idx].dirty = true;
            self.lines[idx].last_used = self.access_counter;
            return Ok(());
        }

        self.misses += 1;
        let idx = self.find_lru();
        self.evict(idx, dev)?;

        self.lines[idx].data[..BLOCK_SIZE].copy_from_slice(&buf[..BLOCK_SIZE]);
        self.lines[idx].lba = lba;
        self.lines[idx].valid = true;
        self.lines[idx].dirty = true;
        self.lines[idx].last_used = self.access_counter;
        Ok(())
    }

    pub fn flush(&mut self, dev: &mut dyn BlockDevice) -> Result<(), BlockError> {
        for i in 0..CACHE_LINES {
            if self.lines[i].valid && self.lines[i].dirty {
                dev.write_block(self.lines[i].lba, &self.lines[i].data)?;
                self.lines[i].dirty = false;
            }
        }
        Ok(())
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}
