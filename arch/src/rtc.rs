#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcError {
    NotAvailable,
    InvalidTime,
    AlarmNotSet,
}

#[derive(Debug, Clone, Copy)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

pub trait RtcDevice {
    fn get_time(&self) -> Result<DateTime, RtcError>;
    fn set_time(&mut self, dt: &DateTime) -> Result<(), RtcError>;
    fn set_alarm(&mut self, dt: &DateTime) -> Result<(), RtcError>;
    fn clear_alarm(&mut self) -> Result<(), RtcError>;
    fn alarm_triggered(&self) -> bool;
}
