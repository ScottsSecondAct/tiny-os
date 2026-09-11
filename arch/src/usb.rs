#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsbError {
    NotAvailable,
    NoDevice,
    TransferFailed,
    Stalled,
    Timeout,
    InvalidEndpoint,
    BufferTooSmall,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
    Super,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferType {
    Control,
    Bulk,
    Interrupt,
    Isochronous,
}

#[derive(Debug, Clone, Copy)]
pub struct UsbDeviceInfo {
    pub port: u8,
    pub speed: UsbSpeed,
    pub vendor_id: u16,
    pub product_id: u16,
    pub class: u8,
    pub subclass: u8,
}

pub trait UsbHostController {
    fn enumerate(&mut self) -> Result<u8, UsbError>;
    fn device_info(&self, dev_id: u8) -> Result<UsbDeviceInfo, UsbError>;
    fn control_transfer(
        &mut self,
        dev_id: u8,
        setup: &[u8; 8],
        data: &mut [u8],
    ) -> Result<usize, UsbError>;
    fn bulk_transfer(
        &mut self,
        dev_id: u8,
        endpoint: u8,
        data: &mut [u8],
    ) -> Result<usize, UsbError>;
    fn interrupt_transfer(
        &mut self,
        dev_id: u8,
        endpoint: u8,
        data: &mut [u8],
    ) -> Result<usize, UsbError>;
}
