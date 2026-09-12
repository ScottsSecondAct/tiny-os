// RP1 I2C0 driver — DesignWare DW_apb_i2c compatible.
//
// Register offsets from RP1_I2C0_BASE:
//   0x00  IC_CON           Control Register
//   0x04  IC_TAR           Target Address Register
//   0x10  IC_DATA_CMD      Data Buffer and Command Register
//   0x14  IC_SS_SCL_HCNT   Standard Speed SCL High Count
//   0x18  IC_SS_SCL_LCNT   Standard Speed SCL Low Count
//   0x1C  IC_FS_SCL_HCNT   Fast Speed SCL High Count
//   0x20  IC_FS_SCL_LCNT   Fast Speed SCL Low Count
//   0x2C  IC_INTR_STAT     Interrupt Status
//   0x30  IC_INTR_MASK     Interrupt Mask
//   0x34  IC_RAW_INTR_STAT Raw Interrupt Status
//   0x3C  IC_RX_TL         RX FIFO Threshold
//   0x40  IC_TX_TL         TX FIFO Threshold
//   0x44  IC_CLR_INTR      Clear Combined Interrupt
//   0x54  IC_CLR_TX_ABRT   Clear TX Abort
//   0x60  IC_ENABLE        Enable Register
//   0x70  IC_STATUS        Status Register
//   0x74  IC_TXFLR         TX FIFO Level
//   0x78  IC_RXFLR         RX FIFO Level
//   0x80  IC_TX_ABRT_SOURCE TX Abort Source

use super::memory_map::RP1_I2C0_BASE;
use arch::i2c::{I2cConfig, I2cDevice, I2cError};

const IC_CON: usize = 0x00;
const IC_TAR: usize = 0x04;
const IC_DATA_CMD: usize = 0x10;
const IC_SS_SCL_HCNT: usize = 0x14;
const IC_SS_SCL_LCNT: usize = 0x18;
const IC_FS_SCL_HCNT: usize = 0x1C;
const IC_FS_SCL_LCNT: usize = 0x20;
const IC_INTR_MASK: usize = 0x30;
const IC_RAW_INTR_STAT: usize = 0x34;
const IC_CLR_INTR: usize = 0x44;
const IC_CLR_TX_ABRT: usize = 0x54;
const IC_ENABLE: usize = 0x60;
const IC_STATUS: usize = 0x70;
const IC_RXFLR: usize = 0x78;
const IC_TX_ABRT_SOURCE: usize = 0x80;

// IC_CON bits
const CON_MASTER_MODE: u32 = 1 << 0;
const CON_SPEED_STD: u32 = 1 << 1; // Standard mode (100 kHz)
const CON_SPEED_FAST: u32 = 2 << 1; // Fast mode (400 kHz)
const CON_SLAVE_DISABLE: u32 = 1 << 6;
const CON_RESTART_EN: u32 = 1 << 5;

// IC_DATA_CMD bits
const DATA_CMD_READ: u32 = 1 << 8;
const DATA_CMD_STOP: u32 = 1 << 9;

// IC_STATUS bits
const STATUS_ACTIVITY: u32 = 1 << 0;
const STATUS_TFNF: u32 = 1 << 1; // TX FIFO Not Full
const STATUS_RFNE: u32 = 1 << 3; // RX FIFO Not Empty

// IC_RAW_INTR_STAT bits
const RAW_TX_ABRT: u32 = 1 << 6;

// RP1 I2C reference clock is 200 MHz
const I2C_REF_CLOCK: u32 = 200_000_000;

pub struct Rp1I2c;

impl Rp1I2c {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_I2C0_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_I2C0_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }

    fn disable(&mut self) {
        Self::write(IC_ENABLE, 0);
        // Wait until disabled
        for _ in 0..10_000 {
            if Self::read(IC_ENABLE) & 1 == 0 {
                return;
            }
        }
    }

    fn enable(&mut self) {
        Self::write(IC_ENABLE, 1);
    }

    fn set_target(&mut self, addr: u8) {
        Self::write(IC_TAR, addr as u32 & 0x7F);
    }

    fn check_abort(&self) -> Result<(), I2cError> {
        if Self::read(IC_RAW_INTR_STAT) & RAW_TX_ABRT != 0 {
            let _source = Self::read(IC_TX_ABRT_SOURCE);
            let _ = Self::read(IC_CLR_TX_ABRT);
            return Err(I2cError::Nack);
        }
        Ok(())
    }

    fn wait_idle(&self) {
        for _ in 0..100_000 {
            if Self::read(IC_STATUS) & STATUS_ACTIVITY == 0 {
                return;
            }
        }
    }
}

impl I2cDevice for Rp1I2c {
    fn configure(&mut self, config: &I2cConfig) -> Result<(), I2cError> {
        self.disable();

        let (speed_bits, scl_hcnt, scl_lcnt) = if config.clock_hz <= 100_000 {
            let _period_ns = 1_000_000_000 / 100_000;
            let hcnt = I2C_REF_CLOCK / 100_000 / 2;
            let lcnt = hcnt;
            (CON_SPEED_STD, hcnt, lcnt)
        } else {
            let hcnt = I2C_REF_CLOCK / 400_000 * 6 / 10; // 60% high
            let lcnt = I2C_REF_CLOCK / 400_000 * 4 / 10; // 40% low
            (CON_SPEED_FAST, hcnt, lcnt)
        };

        let con = CON_MASTER_MODE | speed_bits | CON_SLAVE_DISABLE | CON_RESTART_EN;
        Self::write(IC_CON, con);

        if speed_bits == CON_SPEED_STD {
            Self::write(IC_SS_SCL_HCNT, scl_hcnt);
            Self::write(IC_SS_SCL_LCNT, scl_lcnt);
        } else {
            Self::write(IC_FS_SCL_HCNT, scl_hcnt);
            Self::write(IC_FS_SCL_LCNT, scl_lcnt);
        }

        // Disable all interrupts (polling mode)
        Self::write(IC_INTR_MASK, 0);

        // Clear any pending interrupts
        let _ = Self::read(IC_CLR_INTR);

        self.enable();
        Ok(())
    }

    fn write(&mut self, addr: u8, data: &[u8]) -> Result<usize, I2cError> {
        if data.is_empty() {
            return Ok(0);
        }

        self.disable();
        self.set_target(addr);
        self.enable();

        let _ = Self::read(IC_CLR_INTR);

        for (i, &byte) in data.iter().enumerate() {
            let mut cmd = byte as u32;
            if i == data.len() - 1 {
                cmd |= DATA_CMD_STOP;
            }

            // Wait for TX FIFO space
            for _ in 0..100_000 {
                if Self::read(IC_STATUS) & STATUS_TFNF != 0 {
                    break;
                }
            }

            self.check_abort()?;
            Self::write(IC_DATA_CMD, cmd);
        }

        self.wait_idle();
        self.check_abort()?;
        Ok(data.len())
    }

    fn read(&mut self, addr: u8, buf: &mut [u8]) -> Result<usize, I2cError> {
        if buf.is_empty() {
            return Ok(0);
        }

        self.disable();
        self.set_target(addr);
        self.enable();

        let _ = Self::read(IC_CLR_INTR);

        let mut tx_idx = 0;
        let mut rx_idx = 0;

        while rx_idx < buf.len() {
            // Issue read commands
            while tx_idx < buf.len() && (Self::read(IC_STATUS) & STATUS_TFNF) != 0 {
                let mut cmd = DATA_CMD_READ;
                if tx_idx == buf.len() - 1 {
                    cmd |= DATA_CMD_STOP;
                }
                Self::write(IC_DATA_CMD, cmd);
                tx_idx += 1;
            }

            self.check_abort()?;

            // Read available data
            while rx_idx < buf.len() && (Self::read(IC_STATUS) & STATUS_RFNE) != 0 {
                buf[rx_idx] = Self::read(IC_DATA_CMD) as u8;
                rx_idx += 1;
            }
        }

        self.wait_idle();
        self.check_abort()?;
        Ok(buf.len())
    }

    fn write_read(&mut self, addr: u8, tx: &[u8], rx: &mut [u8]) -> Result<usize, I2cError> {
        if tx.is_empty() && rx.is_empty() {
            return Ok(0);
        }

        self.disable();
        self.set_target(addr);
        self.enable();

        let _ = Self::read(IC_CLR_INTR);

        // Write phase (no STOP — restart will follow)
        for &byte in tx {
            for _ in 0..100_000 {
                if Self::read(IC_STATUS) & STATUS_TFNF != 0 {
                    break;
                }
            }
            self.check_abort()?;
            Self::write(IC_DATA_CMD, byte as u32);
        }

        // Read phase with restart + stop
        let mut tx_idx = 0;
        let mut rx_idx = 0;

        while rx_idx < rx.len() {
            while tx_idx < rx.len() && (Self::read(IC_STATUS) & STATUS_TFNF) != 0 {
                let mut cmd = DATA_CMD_READ;
                if tx_idx == rx.len() - 1 {
                    cmd |= DATA_CMD_STOP;
                }
                Self::write(IC_DATA_CMD, cmd);
                tx_idx += 1;
            }

            self.check_abort()?;

            while rx_idx < rx.len() && (Self::read(IC_STATUS) & STATUS_RFNE) != 0 {
                rx[rx_idx] = Self::read(IC_DATA_CMD) as u8;
                rx_idx += 1;
            }
        }

        self.wait_idle();
        self.check_abort()?;
        Ok(rx.len())
    }
}
