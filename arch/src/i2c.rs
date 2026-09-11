#[derive(Debug, Clone, Copy, PartialEq)]
pub enum I2cError {
    BusNotAvailable,
    Nack,
    ArbitrationLost,
    Timeout,
}

#[derive(Debug, Clone, Copy)]
pub struct I2cConfig {
    pub clock_hz: u32,
}

pub trait I2cDevice {
    fn configure(&mut self, config: &I2cConfig) -> Result<(), I2cError>;
    fn write(&mut self, addr: u8, data: &[u8]) -> Result<usize, I2cError>;
    fn read(&mut self, addr: u8, buf: &mut [u8]) -> Result<usize, I2cError>;
    fn write_read(&mut self, addr: u8, tx: &[u8], rx: &mut [u8]) -> Result<usize, I2cError>;
}
