use crate::storage;
use arch::block::BlockError;

#[derive(Debug)]
pub enum FsError {
    NotFound,
    NotADirectory,
    NotAFile,
    IsADirectory,
    NoSpace,
    InvalidName,
    TooManyOpen,
    BadFd,
    IoError,
    NotMounted,
    CorruptFs,
    ReadOnly,
}

impl From<BlockError> for FsError {
    fn from(_: BlockError) -> Self {
        FsError::IoError
    }
}

pub struct Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub fat_size_32: u32,
    pub root_cluster: u32,
    pub total_sectors_32: u32,
}

pub struct Fat32State {
    pub bpb: Bpb,
    pub partition_lba: u32,
    pub fat_lba: u32,
    pub data_lba: u32,
    pub cluster_count: u32,
    pub mounted: bool,
}

impl Fat32State {
    pub const fn empty() -> Self {
        Self {
            bpb: Bpb {
                bytes_per_sector: 0,
                sectors_per_cluster: 0,
                reserved_sectors: 0,
                num_fats: 0,
                fat_size_32: 0,
                root_cluster: 0,
                total_sectors_32: 0,
            },
            partition_lba: 0,
            fat_lba: 0,
            data_lba: 0,
            cluster_count: 0,
            mounted: false,
        }
    }

    pub fn cluster_to_lba(&self, cluster: u32) -> u64 {
        let offset = (cluster as u64 - 2) * self.bpb.sectors_per_cluster as u64;
        self.data_lba as u64 + offset
    }
}

pub fn mount(state: &mut Fat32State, partition_lba: u32) -> Result<(), FsError> {
    let mut buf = [0u8; 512];
    storage::cached_read(partition_lba as u64, &mut buf)?;

    // Validate boot signature
    if buf[510] != 0x55 || buf[511] != 0xAA {
        return Err(FsError::CorruptFs);
    }

    let bps = u16::from_le_bytes([buf[11], buf[12]]);
    if bps != 512 {
        return Err(FsError::CorruptFs);
    }

    let spc = buf[13];
    if spc == 0 || (spc & (spc - 1)) != 0 {
        return Err(FsError::CorruptFs);
    }

    let reserved = u16::from_le_bytes([buf[14], buf[15]]);
    let num_fats = buf[16];
    let fat_size = u32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]);
    let root_cluster = u32::from_le_bytes([buf[44], buf[45], buf[46], buf[47]]);
    let total_sectors = u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);

    if fat_size == 0 || root_cluster < 2 {
        return Err(FsError::CorruptFs);
    }

    let fat_lba = partition_lba + reserved as u32;
    let data_lba = fat_lba + num_fats as u32 * fat_size;
    let data_sectors = total_sectors - reserved as u32 - num_fats as u32 * fat_size;
    let cluster_count = data_sectors / spc as u32;

    state.bpb = Bpb {
        bytes_per_sector: bps,
        sectors_per_cluster: spc,
        reserved_sectors: reserved,
        num_fats,
        fat_size_32: fat_size,
        root_cluster,
        total_sectors_32: total_sectors,
    };
    state.partition_lba = partition_lba;
    state.fat_lba = fat_lba;
    state.data_lba = data_lba;
    state.cluster_count = cluster_count;
    state.mounted = true;

    Ok(())
}

// --- FAT access ---

pub fn fat_entry(state: &Fat32State, cluster: u32) -> Result<u32, FsError> {
    let fat_offset = cluster as u64 * 4;
    let fat_sector = state.fat_lba as u64 + fat_offset / 512;
    let offset_in_sector = (fat_offset % 512) as usize;

    let mut buf = [0u8; 512];
    storage::cached_read(fat_sector, &mut buf)?;

    let entry = u32::from_le_bytes([
        buf[offset_in_sector],
        buf[offset_in_sector + 1],
        buf[offset_in_sector + 2],
        buf[offset_in_sector + 3],
    ]);
    Ok(entry & 0x0FFF_FFFF)
}

pub fn next_cluster(state: &Fat32State, cluster: u32) -> Result<Option<u32>, FsError> {
    let entry = fat_entry(state, cluster)?;
    if entry >= 0x0FFF_FFF8 {
        Ok(None)
    } else if entry == 0 || entry == 0x0FFF_FFF7 {
        Err(FsError::CorruptFs)
    } else {
        Ok(Some(entry))
    }
}

pub fn set_fat_entry(state: &Fat32State, cluster: u32, value: u32) -> Result<(), FsError> {
    let fat_offset = cluster as u64 * 4;
    let fat_sector = state.fat_lba as u64 + fat_offset / 512;
    let offset_in_sector = (fat_offset % 512) as usize;

    let mut buf = [0u8; 512];
    storage::cached_read(fat_sector, &mut buf)?;

    let existing = u32::from_le_bytes([
        buf[offset_in_sector],
        buf[offset_in_sector + 1],
        buf[offset_in_sector + 2],
        buf[offset_in_sector + 3],
    ]);
    let new_val = (existing & 0xF000_0000) | (value & 0x0FFF_FFFF);
    let bytes = new_val.to_le_bytes();
    buf[offset_in_sector] = bytes[0];
    buf[offset_in_sector + 1] = bytes[1];
    buf[offset_in_sector + 2] = bytes[2];
    buf[offset_in_sector + 3] = bytes[3];

    storage::cached_write(fat_sector, &buf)?;
    Ok(())
}

pub fn alloc_cluster(state: &Fat32State) -> Result<u32, FsError> {
    for c in 2..state.cluster_count + 2 {
        let entry = fat_entry(state, c)?;
        if entry == 0 {
            set_fat_entry(state, c, 0x0FFF_FFFF)?;
            return Ok(c);
        }
    }
    Err(FsError::NoSpace)
}

// --- Directory entry parsing ---

const DIR_ENTRY_SIZE: usize = 32;
const ENTRIES_PER_SECTOR: usize = 512 / DIR_ENTRY_SIZE;

const _ATTR_READ_ONLY: u8 = 0x01;
const _ATTR_HIDDEN: u8 = 0x02;
const _ATTR_SYSTEM: u8 = 0x04;
const _ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const _ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F;

#[derive(Clone, Copy)]
pub struct DirEntry {
    pub name: [u8; 104],
    pub name_len: usize,
    pub size: u32,
    pub is_dir: bool,
    pub cluster: u32,
    pub attr: u8,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            name: [0u8; 104],
            name_len: 0,
            size: 0,
            is_dir: false,
            cluster: 0,
            attr: 0,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

#[derive(Clone, Copy)]
pub struct DirCursor {
    pub cluster: u32,
    pub sector_in_cluster: u8,
    pub entry_in_sector: u8,
    pub finished: bool,
    lfn_buf: [u8; 104],
    lfn_len: usize,
    lfn_seq_expected: u8,
    lfn_checksum: u8,
}

impl DirCursor {
    pub fn new(cluster: u32) -> Self {
        Self {
            cluster,
            sector_in_cluster: 0,
            entry_in_sector: 0,
            finished: false,
            lfn_buf: [0u8; 104],
            lfn_len: 0,
            lfn_seq_expected: 0,
            lfn_checksum: 0,
        }
    }

    fn reset_lfn(&mut self) {
        self.lfn_len = 0;
        self.lfn_seq_expected = 0;
        self.lfn_checksum = 0;
    }
}

pub fn readdir_next(
    state: &Fat32State,
    cursor: &mut DirCursor,
) -> Result<Option<DirEntry>, FsError> {
    if cursor.finished {
        return Ok(None);
    }

    loop {
        let lba = state.cluster_to_lba(cursor.cluster)
            + cursor.sector_in_cluster as u64;

        let mut sector_buf = [0u8; 512];
        storage::cached_read(lba, &mut sector_buf)?;

        while (cursor.entry_in_sector as usize) < ENTRIES_PER_SECTOR {
            let off = cursor.entry_in_sector as usize * DIR_ENTRY_SIZE;
            let raw = &sector_buf[off..off + DIR_ENTRY_SIZE];

            cursor.entry_in_sector += 1;

            if raw[0] == 0x00 {
                cursor.finished = true;
                return Ok(None);
            }

            if raw[0] == 0xE5 {
                cursor.reset_lfn();
                continue;
            }

            let attr = raw[11];

            if attr == ATTR_LFN {
                process_lfn_entry(cursor, raw);
                continue;
            }

            // Regular short entry
            let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]) as u32;
            let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]) as u32;
            let cluster = (cluster_hi << 16) | cluster_lo;
            let size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
            let is_dir = attr & ATTR_DIRECTORY != 0;

            let mut entry = DirEntry::empty();
            entry.cluster = cluster;
            entry.size = size;
            entry.is_dir = is_dir;
            entry.attr = attr;

            if cursor.lfn_len > 0 && lfn_checksum_matches(cursor, &raw[..11]) {
                entry.name[..cursor.lfn_len].copy_from_slice(&cursor.lfn_buf[..cursor.lfn_len]);
                entry.name_len = cursor.lfn_len;
            } else {
                entry.name_len = short_name_to_str(&raw[..11], &mut entry.name);
            }

            cursor.reset_lfn();
            return Ok(Some(entry));
        }

        // Move to next sector in cluster
        cursor.entry_in_sector = 0;
        cursor.sector_in_cluster += 1;

        if cursor.sector_in_cluster >= state.bpb.sectors_per_cluster {
            cursor.sector_in_cluster = 0;
            match next_cluster(state, cursor.cluster)? {
                Some(next) => cursor.cluster = next,
                None => {
                    cursor.finished = true;
                    return Ok(None);
                }
            }
        }
    }
}

fn process_lfn_entry(cursor: &mut DirCursor, raw: &[u8]) {
    let seq = raw[0];
    let is_last = seq & 0x40 != 0;
    let ord = seq & 0x1F;

    if is_last {
        cursor.lfn_buf = [0u8; 104];
        cursor.lfn_len = 0;
        cursor.lfn_seq_expected = ord;
        cursor.lfn_checksum = raw[13];
    } else if ord != cursor.lfn_seq_expected.wrapping_sub(1) || raw[13] != cursor.lfn_checksum {
        cursor.reset_lfn();
        return;
    }

    cursor.lfn_seq_expected = ord;

    // Extract 13 UTF-16LE chars from LFN entry
    let char_offsets: [usize; 13] = [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30];
    let base = (ord as usize - 1) * 13;

    for (i, &off) in char_offsets.iter().enumerate() {
        if off + 1 >= raw.len() {
            break;
        }
        let ch = u16::from_le_bytes([raw[off], raw[off + 1]]);
        if ch == 0x0000 || ch == 0xFFFF {
            break;
        }
        let pos = base + i;
        if pos < 104 {
            // ASCII-only: clamp non-ASCII to '?'
            cursor.lfn_buf[pos] = if ch < 128 { ch as u8 } else { b'?' };
            if pos + 1 > cursor.lfn_len {
                cursor.lfn_len = pos + 1;
            }
        }
    }
}

fn lfn_checksum_matches(cursor: &DirCursor, short_name: &[u8]) -> bool {
    if cursor.lfn_seq_expected != 1 {
        return false;
    }
    let mut sum: u8 = 0;
    for i in 0..11 {
        sum = ((sum >> 1) | (sum << 7)).wrapping_add(short_name[i]);
    }
    sum == cursor.lfn_checksum
}

fn short_name_to_str(raw: &[u8], out: &mut [u8]) -> usize {
    let mut pos = 0;

    // Base name (bytes 0..8), trimmed trailing spaces, lowercased
    for i in 0..8 {
        if raw[i] != b' ' {
            out[pos] = to_lower(raw[i]);
            pos += 1;
        }
    }

    // Extension (bytes 8..11)
    let ext_start = 8;
    let mut has_ext = false;
    for i in ext_start..11 {
        if raw[i] != b' ' {
            if !has_ext {
                out[pos] = b'.';
                pos += 1;
                has_ext = true;
            }
            out[pos] = to_lower(raw[i]);
            pos += 1;
        }
    }

    pos
}

fn to_lower(b: u8) -> u8 {
    if b.is_ascii_uppercase() {
        b + 32
    } else {
        b
    }
}

// --- Path resolution ---

pub fn resolve_path(
    state: &Fat32State,
    path: &str,
) -> Result<(u32, u32, bool, u32, u16), FsError> {
    // Returns (cluster, size, is_dir, parent_cluster, dir_entry_index)
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return Ok((state.bpb.root_cluster, 0, true, 0, 0));
    }

    let mut current_cluster = state.bpb.root_cluster;

    let components: ComponentIter = ComponentIter::new(path);
    let mut parent_cluster = current_cluster;
    let mut entry_idx: u16 = 0;
    let mut last_size: u32 = 0;
    let mut last_is_dir = true;
    let mut last_cluster = current_cluster;

    for component in components {
        if !last_is_dir && component.len() > 0 {
            return Err(FsError::NotADirectory);
        }

        let mut cursor = DirCursor::new(current_cluster);
        let mut found = false;
        let mut idx: u16 = 0;

        while let Some(entry) = readdir_next(state, &mut cursor)? {
            if name_eq(entry.name_str(), component) {
                parent_cluster = current_cluster;
                entry_idx = idx;
                current_cluster = entry.cluster;
                last_size = entry.size;
                last_is_dir = entry.is_dir;
                last_cluster = entry.cluster;
                found = true;
                break;
            }
            idx += 1;
        }

        if !found {
            return Err(FsError::NotFound);
        }
    }

    Ok((last_cluster, last_size, last_is_dir, parent_cluster, entry_idx))
}

fn name_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (ca, cb) in a.bytes().zip(b.bytes()) {
        let la = if ca.is_ascii_uppercase() { ca + 32 } else { ca };
        let lb = if cb.is_ascii_uppercase() { cb + 32 } else { cb };
        if la != lb {
            return false;
        }
    }
    true
}

struct ComponentIter<'a> {
    remaining: &'a str,
}

impl<'a> ComponentIter<'a> {
    fn new(path: &'a str) -> Self {
        Self { remaining: path }
    }
}

impl<'a> Iterator for ComponentIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.remaining.is_empty() {
            return None;
        }
        let (component, rest) = match self.remaining.find('/') {
            Some(idx) => (&self.remaining[..idx], &self.remaining[idx + 1..]),
            None => (self.remaining, ""),
        };
        self.remaining = rest;
        if component.is_empty() {
            self.next()
        } else {
            Some(component)
        }
    }
}

// --- File read ---

pub struct OpenFile {
    pub active: bool,
    pub cluster_start: u32,
    pub size: u32,
    pub position: u32,
    pub is_dir: bool,
    pub writable: bool,
    pub dir_cluster: u32,
    pub dir_entry_idx: u16,
    pub cur_cluster: u32,
    pub cur_cluster_offset: u32,
}

impl OpenFile {
    pub const fn empty() -> Self {
        Self {
            active: false,
            cluster_start: 0,
            size: 0,
            position: 0,
            is_dir: false,
            writable: false,
            dir_cluster: 0,
            dir_entry_idx: 0,
            cur_cluster: 0,
            cur_cluster_offset: 0,
        }
    }
}

pub fn read_file(
    state: &Fat32State,
    file: &mut OpenFile,
    buf: &mut [u8],
) -> Result<usize, FsError> {
    if file.is_dir {
        return Err(FsError::IsADirectory);
    }

    let remaining = file.size.saturating_sub(file.position) as usize;
    if remaining == 0 {
        return Ok(0);
    }

    let to_read = buf.len().min(remaining);
    let bytes_per_cluster = state.bpb.sectors_per_cluster as u32 * 512;
    let mut bytes_read = 0;

    // Seek to the right cluster for current position
    let target_cluster_idx = file.position / bytes_per_cluster;

    let (mut cluster, mut cluster_idx) = if file.cur_cluster != 0
        && file.cur_cluster_offset <= target_cluster_idx
    {
        (file.cur_cluster, file.cur_cluster_offset)
    } else {
        (file.cluster_start, 0)
    };

    while cluster_idx < target_cluster_idx {
        match next_cluster(state, cluster)? {
            Some(next) => {
                cluster = next;
                cluster_idx += 1;
            }
            None => return Ok(0),
        }
    }

    while bytes_read < to_read {
        let offset_in_cluster = (file.position + bytes_read as u32) % bytes_per_cluster;
        let sector_in_cluster = offset_in_cluster / 512;
        let offset_in_sector = (offset_in_cluster % 512) as usize;

        let lba = state.cluster_to_lba(cluster) + sector_in_cluster as u64;
        let mut sector_buf = [0u8; 512];
        storage::cached_read(lba, &mut sector_buf)?;

        let avail_in_sector = 512 - offset_in_sector;
        let want = (to_read - bytes_read).min(avail_in_sector);
        buf[bytes_read..bytes_read + want]
            .copy_from_slice(&sector_buf[offset_in_sector..offset_in_sector + want]);
        bytes_read += want;

        // Check if we've crossed to next cluster
        let new_pos = file.position + bytes_read as u32;
        let new_cluster_idx = new_pos / bytes_per_cluster;
        if new_cluster_idx > cluster_idx && bytes_read < to_read {
            match next_cluster(state, cluster)? {
                Some(next) => {
                    cluster = next;
                    cluster_idx = new_cluster_idx;
                }
                None => break,
            }
        }
    }

    file.position += bytes_read as u32;
    file.cur_cluster = cluster;
    file.cur_cluster_offset = cluster_idx;

    Ok(bytes_read)
}

// --- File write ---

pub fn write_file(
    state: &Fat32State,
    file: &mut OpenFile,
    buf: &[u8],
) -> Result<usize, FsError> {
    if !file.writable {
        return Err(FsError::ReadOnly);
    }

    let bytes_per_cluster = state.bpb.sectors_per_cluster as u32 * 512;
    let mut bytes_written = 0;

    // If file has no clusters yet, allocate the first one
    if file.cluster_start == 0 {
        let c = alloc_cluster(state)?;
        file.cluster_start = c;
        file.cur_cluster = c;
        file.cur_cluster_offset = 0;
    }

    let target_cluster_idx = file.position / bytes_per_cluster;

    let (mut cluster, mut cluster_idx) = if file.cur_cluster != 0
        && file.cur_cluster_offset <= target_cluster_idx
    {
        (file.cur_cluster, file.cur_cluster_offset)
    } else {
        (file.cluster_start, 0)
    };

    // Walk to target cluster, allocating as needed
    while cluster_idx < target_cluster_idx {
        match next_cluster(state, cluster)? {
            Some(next) => {
                cluster = next;
                cluster_idx += 1;
            }
            None => {
                let new_c = alloc_cluster(state)?;
                set_fat_entry(state, cluster, new_c)?;
                cluster = new_c;
                cluster_idx += 1;
            }
        }
    }

    while bytes_written < buf.len() {
        let offset_in_cluster = (file.position + bytes_written as u32) % bytes_per_cluster;
        let sector_in_cluster = offset_in_cluster / 512;
        let offset_in_sector = (offset_in_cluster % 512) as usize;

        let lba = state.cluster_to_lba(cluster) + sector_in_cluster as u64;

        let mut sector_buf = [0u8; 512];
        if offset_in_sector != 0 || (buf.len() - bytes_written) < 512 {
            // Partial sector write — read-modify-write
            storage::cached_read(lba, &mut sector_buf)?;
        }

        let avail_in_sector = 512 - offset_in_sector;
        let want = (buf.len() - bytes_written).min(avail_in_sector);
        sector_buf[offset_in_sector..offset_in_sector + want]
            .copy_from_slice(&buf[bytes_written..bytes_written + want]);
        storage::cached_write(lba, &sector_buf)?;
        bytes_written += want;

        let new_pos = file.position + bytes_written as u32;
        let new_cluster_idx = new_pos / bytes_per_cluster;
        if new_cluster_idx > cluster_idx && bytes_written < buf.len() {
            match next_cluster(state, cluster)? {
                Some(next) => {
                    cluster = next;
                    cluster_idx = new_cluster_idx;
                }
                None => {
                    let new_c = alloc_cluster(state)?;
                    set_fat_entry(state, cluster, new_c)?;
                    cluster = new_c;
                    cluster_idx = new_cluster_idx;
                }
            }
        }
    }

    file.position += bytes_written as u32;
    if file.position > file.size {
        file.size = file.position;
    }

    file.cur_cluster = cluster;
    file.cur_cluster_offset = cluster_idx;

    Ok(bytes_written)
}

// --- File create ---

pub fn create_file(
    state: &Fat32State,
    dir_cluster: u32,
    name: &str,
) -> Result<(u32, u16), FsError> {
    // Convert name to 8.3 format
    let mut short = [b' '; 11];
    if !make_short_name(name, &mut short) {
        return Err(FsError::InvalidName);
    }

    // Find a free directory entry
    let mut cursor_cluster = dir_cluster;
    let mut global_idx: u16 = 0;

    loop {
        for sec in 0..state.bpb.sectors_per_cluster {
            let lba = state.cluster_to_lba(cursor_cluster) + sec as u64;
            let mut buf = [0u8; 512];
            storage::cached_read(lba, &mut buf)?;

            for e in 0..ENTRIES_PER_SECTOR {
                let off = e * DIR_ENTRY_SIZE;
                if buf[off] == 0x00 || buf[off] == 0xE5 {
                    // Allocate a cluster for the new file
                    let file_cluster = alloc_cluster(state)?;

                    // Write directory entry
                    buf[off..off + 11].copy_from_slice(&short);
                    buf[off + 11] = 0x20; // ATTR_ARCHIVE
                    // Zero reserved/time/date fields
                    for b in &mut buf[off + 12..off + 20] {
                        *b = 0;
                    }
                    // Cluster high
                    buf[off + 20] = (file_cluster >> 16) as u8;
                    buf[off + 21] = (file_cluster >> 24) as u8;
                    // Zero time/date
                    for b in &mut buf[off + 22..off + 26] {
                        *b = 0;
                    }
                    // Cluster low
                    buf[off + 26] = file_cluster as u8;
                    buf[off + 27] = (file_cluster >> 8) as u8;
                    // Size = 0
                    buf[off + 28] = 0;
                    buf[off + 29] = 0;
                    buf[off + 30] = 0;
                    buf[off + 31] = 0;

                    storage::cached_write(lba, &buf)?;
                    storage::flush()?;

                    return Ok((file_cluster, global_idx));
                }
                global_idx += 1;
            }
        }

        match next_cluster(state, cursor_cluster)? {
            Some(next) => cursor_cluster = next,
            None => return Err(FsError::NoSpace),
        }
    }
}

pub fn update_dir_entry_size(
    state: &Fat32State,
    dir_cluster: u32,
    entry_idx: u16,
    new_size: u32,
    new_cluster: u32,
) -> Result<(), FsError> {
    let entries_per_cluster =
        state.bpb.sectors_per_cluster as u16 * ENTRIES_PER_SECTOR as u16;
    let cluster_offset = entry_idx / entries_per_cluster;
    let local_idx = (entry_idx % entries_per_cluster) as usize;

    let mut cluster = dir_cluster;
    for _ in 0..cluster_offset {
        cluster = next_cluster(state, cluster)?.ok_or(FsError::CorruptFs)?;
    }

    let sector_in_cluster = local_idx / ENTRIES_PER_SECTOR;
    let entry_in_sector = local_idx % ENTRIES_PER_SECTOR;

    let lba = state.cluster_to_lba(cluster) + sector_in_cluster as u64;
    let mut buf = [0u8; 512];
    storage::cached_read(lba, &mut buf)?;

    let off = entry_in_sector * DIR_ENTRY_SIZE;
    // Update cluster
    buf[off + 20] = (new_cluster >> 16) as u8;
    buf[off + 21] = (new_cluster >> 24) as u8;
    buf[off + 26] = new_cluster as u8;
    buf[off + 27] = (new_cluster >> 8) as u8;
    // Update size
    let size_bytes = new_size.to_le_bytes();
    buf[off + 28] = size_bytes[0];
    buf[off + 29] = size_bytes[1];
    buf[off + 30] = size_bytes[2];
    buf[off + 31] = size_bytes[3];

    storage::cached_write(lba, &buf)?;
    Ok(())
}

fn make_short_name(name: &str, out: &mut [u8; 11]) -> bool {
    let name = name.as_bytes();
    if name.is_empty() || name.len() > 12 {
        return false;
    }

    // Find dot position
    let dot_pos = name.iter().position(|&b| b == b'.');

    let (base, ext) = match dot_pos {
        Some(pos) => (&name[..pos], &name[pos + 1..]),
        None => (name, &[] as &[u8]),
    };

    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return false;
    }

    for (i, &b) in base.iter().enumerate() {
        out[i] = b.to_ascii_uppercase();
    }

    for (i, &b) in ext.iter().enumerate() {
        out[8 + i] = b.to_ascii_uppercase();
    }

    true
}
