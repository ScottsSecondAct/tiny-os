// RP1 USB host controller — xHCI skeleton driver.
//
// xHCI Capability Registers (read-only, from RP1_USB_BASE):
//   0x00   CAPLENGTH/HCIVERSION
//   0x04   HCSPARAMS1  (MaxSlots, MaxIntrs, MaxPorts)
//   0x08   HCSPARAMS2
//   0x10   HCCPARAMS1
//   0x14   DBOFF       Doorbell Offset
//   0x18   RTSOFF      Runtime Register Space Offset
//
// xHCI Operational Registers (at CAPLENGTH offset):
//   0x00   USBCMD      (Run/Stop, HCRST, INTE, HSEE)
//   0x04   USBSTS      (HCH, HSE, EINT, PCD, SRE)
//   0x14   CRCR        Command Ring Control Register
//   0x30   DCBAAP      Device Context Base Address Array Pointer
//   0x38   CONFIG      Max Device Slots Enabled
//
// Port Register Set (0x400 + 0x10 * port_num):
//   0x00   PORTSC      Port Status and Control
//   0x04   PORTPMSC    Port Power Mgmt Status and Control
//   0x08   PORTLI      Port Link Info

use super::memory_map::RP1_USB_BASE;
use arch::usb::{UsbDeviceInfo, UsbError, UsbHostController};

#[allow(dead_code)]
const CAPLENGTH: usize = 0x00;
#[allow(dead_code)]
const HCSPARAMS1: usize = 0x04;
#[allow(dead_code)]
const USBCMD: usize = 0x00; // offset from operational base
#[allow(dead_code)]
const USBSTS: usize = 0x04;
#[allow(dead_code)]
const CONFIG: usize = 0x38;
#[allow(dead_code)]
const PORTSC_BASE: usize = 0x400;

pub struct Rp1Usb;

impl Rp1Usb {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_USB_BASE + offset) as *mut u32
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_USB_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    #[allow(dead_code)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl UsbHostController for Rp1Usb {
    fn enumerate(&mut self) -> Result<u8, UsbError> {
        Err(UsbError::NotAvailable)
    }

    fn device_info(&self, _dev_id: u8) -> Result<UsbDeviceInfo, UsbError> {
        Err(UsbError::NotAvailable)
    }

    fn control_transfer(
        &mut self,
        _dev_id: u8,
        _setup: &[u8; 8],
        _data: &mut [u8],
    ) -> Result<usize, UsbError> {
        Err(UsbError::NotAvailable)
    }

    fn bulk_transfer(
        &mut self,
        _dev_id: u8,
        _endpoint: u8,
        _data: &mut [u8],
    ) -> Result<usize, UsbError> {
        Err(UsbError::NotAvailable)
    }

    fn interrupt_transfer(
        &mut self,
        _dev_id: u8,
        _endpoint: u8,
        _data: &mut [u8],
    ) -> Result<usize, UsbError> {
        Err(UsbError::NotAvailable)
    }
}
