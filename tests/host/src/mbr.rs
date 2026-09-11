/// MBR partition table parsing — mirrors kernel/src/storage/mbr.rs.

const MBR_SIGNATURE: u16 = 0xAA55;
const PARTITION_TABLE_OFFSET: usize = 0x1BE;
const PARTITION_ENTRY_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Partition {
    status: u8,
    part_type: u8,
    lba_start: u32,
    sector_count: u32,
}

impl Partition {
    fn is_active(&self) -> bool {
        self.status == 0x80
    }

    fn size_mb(&self) -> u32 {
        self.sector_count / 2048
    }

    fn type_name(&self) -> &'static str {
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

fn parse_mbr_sector(sector: &[u8; 512]) -> [Option<Partition>; 4] {
    let sig = u16::from_le_bytes([sector[0x1FE], sector[0x1FF]]);
    if sig != MBR_SIGNATURE {
        return [None; 4];
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
    partitions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_mbr(partitions: &[(u8, u8, u32, u32)]) -> [u8; 512] {
        let mut sector = [0u8; 512];
        // MBR signature
        sector[0x1FE] = 0x55;
        sector[0x1FF] = 0xAA;

        for (i, &(status, ptype, lba, count)) in partitions.iter().enumerate() {
            if i >= 4 { break; }
            let offset = PARTITION_TABLE_OFFSET + i * PARTITION_ENTRY_SIZE;
            sector[offset] = status;
            sector[offset + 4] = ptype;
            sector[offset + 8..offset + 12].copy_from_slice(&lba.to_le_bytes());
            sector[offset + 12..offset + 16].copy_from_slice(&count.to_le_bytes());
        }
        sector
    }

    #[test]
    fn parse_empty_mbr() {
        let sector = build_mbr(&[]);
        let parts = parse_mbr_sector(&sector);
        assert!(parts.iter().all(|p| p.is_none()));
    }

    #[test]
    fn parse_single_fat32_partition() {
        let sector = build_mbr(&[(0x80, 0x0C, 2048, 1048576)]);
        let parts = parse_mbr_sector(&sector);
        let p = parts[0].expect("should have partition 0");
        assert!(p.is_active());
        assert_eq!(p.part_type, 0x0C);
        assert_eq!(p.type_name(), "FAT32");
        assert_eq!(p.lba_start, 2048);
        assert_eq!(p.sector_count, 1048576);
        assert_eq!(p.size_mb(), 512);
        assert!(parts[1].is_none());
        assert!(parts[2].is_none());
        assert!(parts[3].is_none());
    }

    #[test]
    fn parse_multiple_partitions() {
        let sector = build_mbr(&[
            (0x80, 0x0C, 2048, 1048576),    // FAT32, active
            (0x00, 0x83, 1050624, 2097152), // Linux, inactive
            (0x00, 0x82, 3147776, 524288),  // Linux swap
        ]);
        let parts = parse_mbr_sector(&sector);
        assert!(parts[0].unwrap().is_active());
        assert!(!parts[1].unwrap().is_active());
        assert_eq!(parts[1].unwrap().type_name(), "Linux");
        assert_eq!(parts[2].unwrap().type_name(), "Linux swap");
        assert!(parts[3].is_none());
    }

    #[test]
    fn parse_invalid_signature() {
        let mut sector = [0u8; 512];
        sector[0x1FE] = 0x00;
        sector[0x1FF] = 0x00;
        // Put a valid partition entry — should still return None due to bad sig.
        let offset = PARTITION_TABLE_OFFSET;
        sector[offset] = 0x80;
        sector[offset + 4] = 0x0C;
        let parts = parse_mbr_sector(&sector);
        assert!(parts.iter().all(|p| p.is_none()));
    }

    #[test]
    fn type_name_coverage() {
        assert_eq!(Partition { status: 0, part_type: 0x01, lba_start: 0, sector_count: 0 }.type_name(), "FAT12");
        assert_eq!(Partition { status: 0, part_type: 0x04, lba_start: 0, sector_count: 0 }.type_name(), "FAT16");
        assert_eq!(Partition { status: 0, part_type: 0x06, lba_start: 0, sector_count: 0 }.type_name(), "FAT16");
        assert_eq!(Partition { status: 0, part_type: 0x0E, lba_start: 0, sector_count: 0 }.type_name(), "FAT16");
        assert_eq!(Partition { status: 0, part_type: 0x0B, lba_start: 0, sector_count: 0 }.type_name(), "FAT32");
        assert_eq!(Partition { status: 0, part_type: 0x07, lba_start: 0, sector_count: 0 }.type_name(), "NTFS/exFAT");
        assert_eq!(Partition { status: 0, part_type: 0xEE, lba_start: 0, sector_count: 0 }.type_name(), "GPT protective");
        assert_eq!(Partition { status: 0, part_type: 0xFF, lba_start: 0, sector_count: 0 }.type_name(), "Unknown");
    }

    #[test]
    fn size_mb_calculation() {
        let p = Partition { status: 0, part_type: 0x0C, lba_start: 0, sector_count: 2048 };
        assert_eq!(p.size_mb(), 1); // 2048 sectors * 512 bytes = 1 MB
        let p2 = Partition { status: 0, part_type: 0x0C, lba_start: 0, sector_count: 2048 * 1024 };
        assert_eq!(p2.size_mb(), 1024);
    }

    #[test]
    fn empty_partition_entries_skipped() {
        let mut sector = build_mbr(&[]);
        // type=0x00 means empty — should be skipped even with other fields set
        let offset = PARTITION_TABLE_OFFSET;
        sector[offset] = 0x80; // active status
        sector[offset + 4] = 0x00; // but type is empty
        sector[offset + 8..offset + 12].copy_from_slice(&2048u32.to_le_bytes());
        sector[offset + 12..offset + 16].copy_from_slice(&1024u32.to_le_bytes());
        let parts = parse_mbr_sector(&sector);
        assert!(parts[0].is_none());
    }
}
