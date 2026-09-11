use arch::block::{BlockDevice, BlockError};
use core::cell::UnsafeCell;

const SECTOR_SIZE: usize = 512;
const SECTOR_COUNT: usize = 512;
const DISK_SIZE: usize = SECTOR_SIZE * SECTOR_COUNT;

static mut DISK: [u8; DISK_SIZE] = [0u8; DISK_SIZE];
static mut INITIALIZED: bool = false;

pub struct RamDisk;

struct RamDiskCell(UnsafeCell<RamDisk>);
// SAFETY: Single-task access enforced by caller.
unsafe impl Sync for RamDiskCell {}

static RAMDISK: RamDiskCell = RamDiskCell(UnsafeCell::new(RamDisk));

impl BlockDevice for RamDisk {
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        let lba = lba as usize;
        if lba >= SECTOR_COUNT {
            return Err(BlockError::InvalidLba);
        }
        let offset = lba * SECTOR_SIZE;
        // SAFETY: Single-task access, bounds checked above.
        let disk = unsafe { &*core::ptr::addr_of!(DISK) };
        buf[..SECTOR_SIZE].copy_from_slice(&disk[offset..offset + SECTOR_SIZE]);
        Ok(())
    }

    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError> {
        let lba = lba as usize;
        if lba >= SECTOR_COUNT {
            return Err(BlockError::InvalidLba);
        }
        let offset = lba * SECTOR_SIZE;
        // SAFETY: Single-task access, bounds checked above.
        let disk = unsafe { &mut *core::ptr::addr_of_mut!(DISK) };
        disk[offset..offset + SECTOR_SIZE].copy_from_slice(&buf[..SECTOR_SIZE]);
        Ok(())
    }

    fn block_count(&self) -> u64 {
        SECTOR_COUNT as u64
    }

    fn block_size(&self) -> usize {
        SECTOR_SIZE
    }
}

pub fn device() -> &'static mut dyn BlockDevice {
    // SAFETY: Single-task access.
    unsafe { &mut *RAMDISK.0.get() }
}

pub fn is_initialized() -> bool {
    // SAFETY: Written once during init, read after.
    unsafe { INITIALIZED }
}

pub fn init() {
    // SAFETY: Called once from kmain before scheduler starts.
    let disk = unsafe { &mut *core::ptr::addr_of_mut!(DISK) };

    // --- MBR (sector 0) ---
    let mbr = &mut disk[0..SECTOR_SIZE];
    // Partition 1: FAT32 LBA (type 0x0C), starting at sector 1, 511 sectors
    mbr[0x1BE] = 0x80; // active
    mbr[0x1C2] = 0x0C; // FAT32 LBA
    // LBA start = 1
    mbr[0x1C6] = 1;
    mbr[0x1C7] = 0;
    mbr[0x1C8] = 0;
    mbr[0x1C9] = 0;
    // Sector count = 511
    let count = (SECTOR_COUNT - 1) as u32;
    mbr[0x1CA] = count as u8;
    mbr[0x1CB] = (count >> 8) as u8;
    mbr[0x1CC] = (count >> 16) as u8;
    mbr[0x1CD] = (count >> 24) as u8;
    // MBR signature
    mbr[0x1FE] = 0x55;
    mbr[0x1FF] = 0xAA;

    // --- BPB (sector 1 = partition start) ---
    // FAT32 BPB layout for a tiny volume:
    //   Reserved sectors: 1 (just the BPB itself)
    //   FAT count: 1
    //   FAT size: 4 sectors (covers 512 entries = 512 clusters)
    //   Sectors per cluster: 1
    //   Data starts at: partition_lba(1) + reserved(1) + fat_size(4) = sector 6
    //   Root cluster: 2 (first data cluster)
    //   Total data sectors: 511 - 1 - 4 = 506
    //   Total clusters: 506

    let bpb = &mut disk[SECTOR_SIZE..SECTOR_SIZE * 2];

    // Jump boot code
    bpb[0] = 0xEB;
    bpb[1] = 0x58;
    bpb[2] = 0x90;

    // OEM name
    bpb[3..11].copy_from_slice(b"TINYOS  ");

    // Bytes per sector = 512
    bpb[11] = 0x00;
    bpb[12] = 0x02;

    // Sectors per cluster = 1
    bpb[13] = 1;

    // Reserved sector count = 1
    bpb[14] = 1;
    bpb[15] = 0;

    // Number of FATs = 1
    bpb[16] = 1;

    // Root entry count = 0 (FAT32)
    bpb[17] = 0;
    bpb[18] = 0;

    // Total sectors 16 = 0 (use 32-bit field)
    bpb[19] = 0;
    bpb[20] = 0;

    // Media type = 0xF8 (fixed disk)
    bpb[21] = 0xF8;

    // FAT size 16 = 0 (use 32-bit field)
    bpb[22] = 0;
    bpb[23] = 0;

    // Sectors per track = 1
    bpb[24] = 1;
    bpb[25] = 0;

    // Number of heads = 1
    bpb[26] = 1;
    bpb[27] = 0;

    // Hidden sectors = 1 (partition starts at LBA 1)
    bpb[28] = 1;
    bpb[29] = 0;
    bpb[30] = 0;
    bpb[31] = 0;

    // Total sectors 32 = 511
    let total = (SECTOR_COUNT - 1) as u32;
    bpb[32] = total as u8;
    bpb[33] = (total >> 8) as u8;
    bpb[34] = (total >> 16) as u8;
    bpb[35] = (total >> 24) as u8;

    // FAT size 32 = 4 sectors
    bpb[36] = 4;
    bpb[37] = 0;
    bpb[38] = 0;
    bpb[39] = 0;

    // Ext flags = 0
    bpb[40] = 0;
    bpb[41] = 0;

    // FS version = 0.0
    bpb[42] = 0;
    bpb[43] = 0;

    // Root cluster = 2
    bpb[44] = 2;
    bpb[45] = 0;
    bpb[46] = 0;
    bpb[47] = 0;

    // FS info sector = 0 (not used)
    bpb[48] = 0;
    bpb[49] = 0;

    // Backup boot sector = 0 (not used)
    bpb[50] = 0;
    bpb[51] = 0;

    // Boot signature
    bpb[510] = 0x55;
    bpb[511] = 0xAA;

    // --- FAT (sectors 2..5, relative to disk; partition-relative sectors 1..4) ---
    // FAT starts at disk sector 2 (partition_lba=1, reserved=1, so disk sector 1+1=2)
    let fat_offset = 2 * SECTOR_SIZE;
    let fat = &mut disk[fat_offset..];

    // Entry 0: media type marker
    fat[0] = 0xF8;
    fat[1] = 0xFF;
    fat[2] = 0xFF;
    fat[3] = 0x0F;

    // Entry 1: end-of-chain marker
    fat[4] = 0xFF;
    fat[5] = 0xFF;
    fat[6] = 0xFF;
    fat[7] = 0x0F;

    // Entry 2: root directory cluster — end-of-chain (1 cluster = 1 sector)
    fat[8] = 0xFF;
    fat[9] = 0xFF;
    fat[10] = 0xFF;
    fat[11] = 0x0F;

    // Entry 3: HELLO.TXT — end-of-chain (fits in 1 cluster)
    fat[12] = 0xFF;
    fat[13] = 0xFF;
    fat[14] = 0xFF;
    fat[15] = 0x0F;

    // Entry 4: README.TXT — end-of-chain
    fat[16] = 0xFF;
    fat[17] = 0xFF;
    fat[18] = 0xFF;
    fat[19] = 0x0F;

    // --- Root directory (cluster 2 → data sector 0 → disk sector 6) ---
    // Data region starts at: partition_lba(1) + reserved(1) + fat_size(4) = disk sector 6
    // Cluster 2 → disk sector 6
    let root_offset = 6 * SECTOR_SIZE;
    let root = &mut disk[root_offset..root_offset + SECTOR_SIZE];

    // Entry 0: HELLO.TXT (cluster 3)
    write_dir_entry(root, 0, b"HELLO   TXT", 0x20, 3, 20);

    // Entry 1: README.TXT (cluster 4)
    write_dir_entry(root, 1, b"README  TXT", 0x20, 4, 35);

    // --- File data ---
    // HELLO.TXT at cluster 3 → disk sector 7
    let hello = b"Hello from tiny_os!\n";
    let hello_offset = 7 * SECTOR_SIZE;
    disk[hello_offset..hello_offset + hello.len()].copy_from_slice(hello);

    // README.TXT at cluster 4 → disk sector 8
    let readme = b"tiny_os ramdisk test file content.\n\0";
    let readme_offset = 8 * SECTOR_SIZE;
    disk[readme_offset..readme_offset + readme.len()].copy_from_slice(readme);

    // SAFETY: Written once during init.
    unsafe { INITIALIZED = true };
}

fn write_dir_entry(dir: &mut [u8], idx: usize, name: &[u8; 11], attr: u8, cluster: u16, size: u32) {
    let off = idx * 32;
    dir[off..off + 11].copy_from_slice(name);
    dir[off + 11] = attr;
    // Cluster high = 0
    dir[off + 20] = 0;
    dir[off + 21] = 0;
    // Cluster low
    dir[off + 26] = cluster as u8;
    dir[off + 27] = (cluster >> 8) as u8;
    // File size
    dir[off + 28] = size as u8;
    dir[off + 29] = (size >> 8) as u8;
    dir[off + 30] = (size >> 16) as u8;
    dir[off + 31] = (size >> 24) as u8;
}
