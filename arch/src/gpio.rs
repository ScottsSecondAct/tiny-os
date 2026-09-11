#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GpioError {
    InvalidPin,
    PinInUse,
    NotConfigured,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinMode {
    Input,
    Output,
    AltFunc0,
    AltFunc1,
    AltFunc2,
    AltFunc3,
    AltFunc4,
    AltFunc5,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PullMode {
    None,
    Up,
    Down,
}

pub trait GpioController {
    fn set_mode(&mut self, pin: u8, mode: PinMode) -> Result<(), GpioError>;
    fn set_pull(&mut self, pin: u8, pull: PullMode) -> Result<(), GpioError>;
    fn read(&self, pin: u8) -> Result<bool, GpioError>;
    fn write(&mut self, pin: u8, high: bool) -> Result<(), GpioError>;
}
