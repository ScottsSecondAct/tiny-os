use arch::block::{BlockDevice, BlockError};

const MBR_SIGNATURE: u16 = 0xAA55;
const PARTITION_TABLE_OFFSET: usize = 0x1BE;
const PARTITION_ENTRY_SIZE: usize = 16;

#[derive(Clone, Copy)]
pub struct Partition {
    pub status: u8,
    pub part_type: u8,
    pub lba_start: u32,
    pub sector_count: u32,
}

impl Partition {
    pub fn is_active(&self) -> bool {
        self.status == 0x80
    }

    pub fn size_mb(&self) -> u32 {
        self.sector_count / 2048
    }

    pub fn type_name(&self) -> &'static str {
        match self.part_type {
            0x00 => "Empty",
            0x01 => "FAT12",
            0x04 | 0x06 | 0x0E => "FAT16",
            0x0B | 0x0C => "FAT32",
            0x07 => "NTFS/exFAT",
            0x82 => "Linux swap",
            0x83 => "Linux",
            0xEE => "GPT protective",
            _ => "Unknown",
        }
    }
}

pub fn parse_mbr(dev: &mut dyn BlockDevice) -> Result<[Option<Partition>; 4], BlockError> {
    let mut sector = [0u8; 512];
    dev.read_block(0, &mut sector)?;

    let sig = u16::from_le_bytes([sector[0x1FE], sector[0x1FF]]);
    if sig != MBR_SIGNATURE {
        return Ok([None; 4]);
    }

    let mut partitions = [None; 4];
    for i in 0..4 {
        let offset = PARTITION_TABLE_OFFSET + i * PARTITION_ENTRY_SIZE;
        let entry = &sector[offset..offset + PARTITION_ENTRY_SIZE];

        let part_type = entry[4];
        if part_type == 0x00 {
            continue;
        }

        partitions[i] = Some(Partition {
            status: entry[0],
            part_type,
            lba_start: u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]),
            sector_count: u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]),
        });
    }

    Ok(partitions)
}
