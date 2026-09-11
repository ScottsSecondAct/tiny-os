#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SerialError {
    PortNotAvailable,
    InvalidConfig,
    BufferOverflow,
    FramingError,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Parity {
    None,
    Odd,
    Even,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopBits {
    One,
    Two,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlowControl {
    None,
    RtsCts,
}

#[derive(Debug, Clone, Copy)]
pub struct SerialConfig {
    pub port: u8,
    pub baud_rate: u32,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

pub trait SerialPort {
    fn open(&mut self, config: &SerialConfig) -> Result<(), SerialError>;
    fn write(&mut self, port: u8, data: &[u8]) -> Result<usize, SerialError>;
    fn read(&mut self, port: u8, buf: &mut [u8]) -> Result<usize, SerialError>;
    fn close(&mut self, port: u8) -> Result<(), SerialError>;
    fn bytes_available(&self, port: u8) -> Result<usize, SerialError>;
}
