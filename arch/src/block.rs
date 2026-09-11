#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockError {
    Timeout,
    Crc,
    NoMedia,
    WriteProtected,
    InvalidLba,
    IoError,
}

pub trait BlockDevice {
    fn read_block(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), BlockError>;
    fn write_block(&mut self, lba: u64, buf: &[u8]) -> Result<(), BlockError>;
    fn block_count(&self) -> u64;
    fn block_size(&self) -> usize;
}
