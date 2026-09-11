#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpiError {
    BusNotAvailable,
    TransferFailed,
    InvalidConfig,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpiMode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

#[derive(Debug, Clone, Copy)]
pub struct SpiConfig {
    pub clock_hz: u32,
    pub mode: SpiMode,
    pub bits_per_word: u8,
    pub cs_pin: u8,
}

pub trait SpiDevice {
    fn configure(&mut self, config: &SpiConfig) -> Result<(), SpiError>;
    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<usize, SpiError>;
    fn write(&mut self, data: &[u8]) -> Result<usize, SpiError>;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, SpiError>;
}
