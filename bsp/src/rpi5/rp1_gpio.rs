// RP1 GPIO driver.
//
// The RP1 has 28 GPIO pins (GPIO0-GPIO27). Each pin has:
//   - A status register (read-only, shows pin state)
//   - A control register (function select, pull, drive, etc.)
//
// Register layout per pin (stride = 8 bytes):
//   0x00 + pin*8  GPIO_STATUS  (read-only: level, edge, interrupt state)
//   0x04 + pin*8  GPIO_CTRL    (function select, output override, etc.)
//
// Additional registers:
//   0x100 + bank*4  PADS_CTRL   Pad control (pull, drive, slew, schmitt)
//
// The RP1 GPIO bank register block:
//   0x0000  RIO (Register I/O) base — direct pin read/write
//     +0x00  OUT     output value
//     +0x04  OE      output enable
//     +0x08  IN      input value
//
// RIO register set/clr/xor offsets:
//   +0x2000  SET variant (write 1 to set bits)
//   +0x3000  CLR variant (write 1 to clear bits)

use super::memory_map::RP1_GPIO_BASE;
use arch::gpio::{GpioController, GpioError, PinMode, PullMode};

const MAX_PINS: u8 = 28;

// GPIO control register offsets (per-pin, stride 8)
const GPIO_CTRL_OFFSET: usize = 0x04;
const GPIO_STRIDE: usize = 0x08;

// Pad control base (separate block within GPIO region)
const PADS_BASE_OFFSET: usize = 0x100;
const PAD_STRIDE: usize = 0x04;

// RIO (Register I/O) base within GPIO region
const RIO_BASE: usize = 0x0000;
const RIO_OUT: usize = RIO_BASE + 0x00;
const RIO_OE: usize = RIO_BASE + 0x04;
const RIO_IN: usize = RIO_BASE + 0x08;
const RIO_SET: usize = 0x2000;
const RIO_CLR: usize = 0x3000;

// GPIO_CTRL function select field (bits 4:0)
const CTRL_FUNCSEL_MASK: u32 = 0x1F;
const FUNCSEL_NULL: u32 = 31;  // Disconnect (hi-Z)
const FUNCSEL_SIO: u32 = 5;   // Software I/O (GPIO mode)
const FUNCSEL_ALT0: u32 = 0;
const FUNCSEL_ALT1: u32 = 1;
const FUNCSEL_ALT2: u32 = 2;
const FUNCSEL_ALT3: u32 = 3;
const FUNCSEL_ALT4: u32 = 4;
const FUNCSEL_ALT5: u32 = 5;

// Pad control bits
const PAD_PUE: u32 = 1 << 3; // Pull-up enable
const PAD_PDE: u32 = 1 << 2; // Pull-down enable
const PAD_IE: u32 = 1 << 6;  // Input enable
const PAD_OD: u32 = 1 << 7;  // Output disable

pub struct Rp1Gpio;

impl Rp1Gpio {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_GPIO_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read_reg(offset: usize) -> u32 {
        // SAFETY: RP1_GPIO_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write_reg(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }

    fn validate_pin(pin: u8) -> Result<(), GpioError> {
        if pin >= MAX_PINS {
            Err(GpioError::InvalidPin)
        } else {
            Ok(())
        }
    }

    fn ctrl_offset(pin: u8) -> usize {
        (pin as usize) * GPIO_STRIDE + GPIO_CTRL_OFFSET
    }

    fn pad_offset(pin: u8) -> usize {
        PADS_BASE_OFFSET + (pin as usize) * PAD_STRIDE
    }
}

impl GpioController for Rp1Gpio {
    fn set_mode(&mut self, pin: u8, mode: PinMode) -> Result<(), GpioError> {
        Self::validate_pin(pin)?;

        let funcsel = match mode {
            PinMode::Input => FUNCSEL_SIO,
            PinMode::Output => FUNCSEL_SIO,
            PinMode::AltFunc0 => FUNCSEL_ALT0,
            PinMode::AltFunc1 => FUNCSEL_ALT1,
            PinMode::AltFunc2 => FUNCSEL_ALT2,
            PinMode::AltFunc3 => FUNCSEL_ALT3,
            PinMode::AltFunc4 => FUNCSEL_ALT4,
            PinMode::AltFunc5 => FUNCSEL_ALT5,
        };

        // Set function select in GPIO_CTRL
        let ctrl = Self::ctrl_offset(pin);
        let val = (Self::read_reg(ctrl) & !CTRL_FUNCSEL_MASK) | funcsel;
        Self::write_reg(ctrl, val);

        let pin_mask = 1u32 << pin;

        match mode {
            PinMode::Input => {
                // Clear output enable, enable input in pad
                Self::write_reg(RIO_OE + RIO_CLR, pin_mask);
                let pad = Self::pad_offset(pin);
                let pv = (Self::read_reg(pad) | PAD_IE) & !PAD_OD;
                Self::write_reg(pad, pv);
            }
            PinMode::Output => {
                // Set output enable, disable input in pad
                Self::write_reg(RIO_OE + RIO_SET, pin_mask);
                let pad = Self::pad_offset(pin);
                let pv = (Self::read_reg(pad) & !PAD_OD) | PAD_IE;
                Self::write_reg(pad, pv);
            }
            _ => {
                // Alt function — pad settings depend on the peripheral
                let pad = Self::pad_offset(pin);
                let pv = Self::read_reg(pad) | PAD_IE;
                Self::write_reg(pad, pv);
            }
        }

        Ok(())
    }

    fn set_pull(&mut self, pin: u8, pull: PullMode) -> Result<(), GpioError> {
        Self::validate_pin(pin)?;

        let pad = Self::pad_offset(pin);
        let mut val = Self::read_reg(pad) & !(PAD_PUE | PAD_PDE);

        match pull {
            PullMode::None => {}
            PullMode::Up => val |= PAD_PUE,
            PullMode::Down => val |= PAD_PDE,
        }

        Self::write_reg(pad, val);
        Ok(())
    }

    fn read(&self, pin: u8) -> Result<bool, GpioError> {
        Self::validate_pin(pin)?;
        let val = Self::read_reg(RIO_IN);
        Ok((val >> pin) & 1 != 0)
    }

    fn write(&mut self, pin: u8, high: bool) -> Result<(), GpioError> {
        Self::validate_pin(pin)?;
        let pin_mask = 1u32 << pin;
        if high {
            Self::write_reg(RIO_OUT + RIO_SET, pin_mask);
        } else {
            Self::write_reg(RIO_OUT + RIO_CLR, pin_mask);
        }
        Ok(())
    }
}
