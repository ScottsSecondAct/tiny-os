#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DmaError {
    InvalidChannel,
    Busy,
    TransferError,
}

pub trait DmaEngine {
    fn configure_channel(
        &mut self,
        channel: u8,
        src: usize,
        dst: usize,
        len: usize,
    ) -> Result<(), DmaError>;
    fn start_transfer(&mut self, channel: u8) -> Result<(), DmaError>;
    fn is_complete(&self, channel: u8) -> bool;
    fn abort(&mut self, channel: u8);
}
