// RP1 PWM controller driver — 2-channel PWM.
//
// Register offsets from RP1_PWM_BASE:
//   0x00  CS     Control/Status (PWEN1, MODE1, MSEN1, PWEN2, MODE2, MSEN2)
//   0x04  STA    Status (FULL1, EMPT1, WERR1, BERR1, STA1, ...)
//   0x08  DMAC   DMA Control (not used — polling mode)
//   0x10  RNG1   Channel 1 Range (period in PWM clock ticks)
//   0x14  DAT1   Channel 1 Data (duty cycle in ticks)
//   0x20  RNG2   Channel 2 Range
//   0x24  DAT2   Channel 2 Data

use super::memory_map::RP1_PWM_BASE;
use arch::pwm::{PwmConfig, PwmDevice, PwmError};

const CS: usize = 0x00;
const RNG1: usize = 0x10;
const DAT1: usize = 0x14;
const RNG2: usize = 0x20;
const DAT2: usize = 0x24;

// CS register bits
const CS_PWEN1: u32 = 1 << 0;
const CS_MODE1: u32 = 1 << 1;
const CS_MSEN1: u32 = 1 << 7;
const CS_PWEN2: u32 = 1 << 8;
const CS_MODE2: u32 = 1 << 9;
const CS_MSEN2: u32 = 1 << 15;

// RP1 PWM reference clock (50 MHz from crystal PLL)
const PWM_REF_CLOCK: u32 = 50_000_000;

pub struct Rp1Pwm;

impl Rp1Pwm {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_PWM_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_PWM_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl PwmDevice for Rp1Pwm {
    fn configure(&mut self, config: &PwmConfig) -> Result<(), PwmError> {
        if config.channel > 1 {
            return Err(PwmError::InvalidChannel);
        }
        if config.duty_percent > 100 || config.frequency_hz == 0 {
            return Err(PwmError::InvalidConfig);
        }

        let range = PWM_REF_CLOCK / config.frequency_hz;
        if range == 0 {
            return Err(PwmError::InvalidConfig);
        }
        let data = range * config.duty_percent as u32 / 100;

        let (rng_off, dat_off, mode_bits) = if config.channel == 0 {
            (RNG1, DAT1, CS_MODE1 | CS_MSEN1)
        } else {
            (RNG2, DAT2, CS_MODE2 | CS_MSEN2)
        };

        Self::write(rng_off, range);
        Self::write(dat_off, data);

        let cs = Self::read(CS);
        Self::write(CS, cs | mode_bits);

        Ok(())
    }

    fn set_duty(&mut self, channel: u8, duty_percent: u8) -> Result<(), PwmError> {
        if channel > 1 {
            return Err(PwmError::InvalidChannel);
        }
        if duty_percent > 100 {
            return Err(PwmError::InvalidConfig);
        }

        let (rng_off, dat_off) = if channel == 0 {
            (RNG1, DAT1)
        } else {
            (RNG2, DAT2)
        };

        let range = Self::read(rng_off);
        if range == 0 {
            return Err(PwmError::NotEnabled);
        }
        let data = range * duty_percent as u32 / 100;
        Self::write(dat_off, data);

        Ok(())
    }

    fn enable(&mut self, channel: u8) -> Result<(), PwmError> {
        if channel > 1 {
            return Err(PwmError::InvalidChannel);
        }

        let pwen = if channel == 0 { CS_PWEN1 } else { CS_PWEN2 };
        let cs = Self::read(CS);
        Self::write(CS, cs | pwen);

        Ok(())
    }

    fn disable(&mut self, channel: u8) -> Result<(), PwmError> {
        if channel > 1 {
            return Err(PwmError::InvalidChannel);
        }

        let pwen = if channel == 0 { CS_PWEN1 } else { CS_PWEN2 };
        let cs = Self::read(CS);
        Self::write(CS, cs & !pwen);

        Ok(())
    }

    fn get_duty(&self, channel: u8) -> Result<u8, PwmError> {
        if channel > 1 {
            return Err(PwmError::InvalidChannel);
        }

        let (rng_off, dat_off) = if channel == 0 {
            (RNG1, DAT1)
        } else {
            (RNG2, DAT2)
        };

        let range = Self::read(rng_off);
        if range == 0 {
            return Ok(0);
        }
        let data = Self::read(dat_off);
        Ok((data * 100 / range) as u8)
    }
}
