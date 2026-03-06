/// Trait for UART drivers. Implementing `core::fmt::Write` allows use with `write!`.
pub trait UartDriver: core::fmt::Write {
    fn init(&mut self);
    fn write_byte(&mut self, byte: u8);
}
