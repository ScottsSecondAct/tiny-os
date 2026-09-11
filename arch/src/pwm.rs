#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PwmError {
    InvalidChannel,
    InvalidConfig,
    NotEnabled,
    HardwareNotAvailable,
}

#[derive(Debug, Clone, Copy)]
pub struct PwmConfig {
    pub channel: u8,
    pub frequency_hz: u32,
    pub duty_percent: u8,
}

pub trait PwmDevice {
    fn configure(&mut self, config: &PwmConfig) -> Result<(), PwmError>;
    fn set_duty(&mut self, channel: u8, duty_percent: u8) -> Result<(), PwmError>;
    fn enable(&mut self, channel: u8) -> Result<(), PwmError>;
    fn disable(&mut self, channel: u8) -> Result<(), PwmError>;
    fn get_duty(&self, channel: u8) -> Result<u8, PwmError>;
}
