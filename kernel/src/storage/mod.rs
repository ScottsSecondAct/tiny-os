pub mod mbr;
pub mod cache;

use arch::aarch64::emmc2;
use arch::block::BlockDevice;

struct Emmc2Wrapper;

impl BlockDevice for Emmc2Wrapper {
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), arch::block::BlockError> {
        emmc2::read_block(lba, buf)
    }
    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), arch::block::BlockError> {
        emmc2::write_block(lba, buf)
    }
    fn block_count(&self) -> u64 {
        emmc2::card_info().map(|(_, c)| c).unwrap_or(0)
    }
    fn block_size(&self) -> usize {
        512
    }
}

pub fn print_mbr_info() {
    let mut dev = Emmc2Wrapper;
    match mbr::parse_mbr(&mut dev) {
        Ok(parts) => {
            let mut found = false;
            for (i, p) in parts.iter().enumerate() {
                if let Some(part) = p {
                    found = true;
                    crate::kprintln!(
                        "  partition {}: {} {} LBA {}..{} ({} MB)",
                        i,
                        part.type_name(),
                        if part.is_active() { "*" } else { " " },
                        part.lba_start,
                        part.lba_start + part.sector_count,
                        part.size_mb()
                    );
                }
            }
            if !found {
                crate::kprintln!("  no partitions found");
            }
        }
        Err(_) => {
            crate::kprintln!("  failed to read MBR");
        }
    }
}
