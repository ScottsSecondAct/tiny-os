#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NetError {
    NoLink,
    QueueFull,
    InvalidBuf,
    NoRoute,
    Timeout,
    Refused,
    NotConnected,
    AddrInUse,
    BadFd,
}

pub trait NetDevice {
    fn send(&mut self, buf_idx: u16) -> Result<(), NetError>;
    fn recv(&mut self) -> Option<u16>;
    fn mac_addr(&self) -> [u8; 6];
    fn has_link(&self) -> bool;
}
