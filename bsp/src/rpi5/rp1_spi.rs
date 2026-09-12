// RP1 SPI0 driver — DesignWare DW_apb_ssi compatible.
//
// Register offsets from RP1_SPI0_BASE:
//   0x00  CTRLR0   Control Register 0 (frame format, data size, mode)
//   0x04  CTRLR1   Control Register 1 (number of data frames for RX)
//   0x08  SSIENR   SSI Enable (0=disable, 1=enable)
//   0x0C  MWCR     Microwire Control (unused)
//   0x10  SER      Slave Enable Register (bit per CS line)
//   0x14  BAUDR    Baud Rate Select (even divisor of ssi_clk)
//   0x18  TXFTLR   TX FIFO Threshold Level
//   0x1C  RXFTLR   RX FIFO Threshold Level
//   0x20  TXFLR    TX FIFO Level (current count)
//   0x24  RXFLR    RX FIFO Level (current count)
//   0x28  SR       Status Register
//   0x2C  IMR      Interrupt Mask Register
//   0x30  ISR      Interrupt Status Register
//   0x34  RISR     Raw Interrupt Status Register
//   0x38  TXOICR   TX FIFO Overflow Interrupt Clear
//   0x3C  RXOICR   RX FIFO Overflow Interrupt Clear
//   0x40  RXUICR   RX FIFO Underflow Interrupt Clear
//   0x44  MSTICR   Multi-Master Interrupt Clear
//   0x48  ICR      Interrupt Clear Register
//   0x60  DR       Data Register (read/write FIFO)

use super::memory_map::RP1_SPI0_BASE;
use arch::spi::{SpiConfig, SpiDevice, SpiError, SpiMode};

const CTRLR0: usize = 0x00;
const SSIENR: usize = 0x08;
const SER: usize = 0x10;
const BAUDR: usize = 0x14;
const TXFLR: usize = 0x20;
const RXFLR: usize = 0x24;
const SR: usize = 0x28;
const IMR: usize = 0x2C;
const DR: usize = 0x60;

// Status Register bits
const SR_BUSY: u32 = 1 << 0;
const SR_TFNF: u32 = 1 << 1; // TX FIFO Not Full
const SR_TFE: u32 = 1 << 2; // TX FIFO Empty
const SR_RFNE: u32 = 1 << 3; // RX FIFO Not Empty

// CTRLR0 fields
const CTRLR0_DFS_MASK: u32 = 0xF;
const CTRLR0_SCPH: u32 = 1 << 6; // Serial Clock Phase
const CTRLR0_SCPOL: u32 = 1 << 7; // Serial Clock Polarity
const CTRLR0_TMOD_TR: u32 = 0 << 8; // TX and RX mode

// RP1 SPI reference clock is 200 MHz
const SPI_REF_CLOCK: u32 = 200_000_000;

const FIFO_DEPTH: usize = 16;

pub struct Rp1Spi;

impl Rp1Spi {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_SPI0_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_SPI0_BASE is a valid MMIO address within the RP1
        // peripheral window, which is identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }

    fn disable(&mut self) {
        Self::write(SSIENR, 0);
    }

    fn enable(&mut self) {
        Self::write(SSIENR, 1);
    }

    fn wait_idle(&self) {
        while Self::read(SR) & SR_BUSY != 0 {}
    }

    fn flush_fifos(&mut self) {
        self.disable();
        while Self::read(RXFLR) > 0 {
            Self::read(DR);
        }
    }
}

impl SpiDevice for Rp1Spi {
    fn configure(&mut self, config: &SpiConfig) -> Result<(), SpiError> {
        if config.clock_hz == 0 || config.clock_hz > SPI_REF_CLOCK / 2 {
            return Err(SpiError::InvalidConfig);
        }
        if config.bits_per_word < 4 || config.bits_per_word > 16 {
            return Err(SpiError::InvalidConfig);
        }

        self.disable();

        let dfs = (config.bits_per_word as u32 - 1) & CTRLR0_DFS_MASK;
        let (cpol, cpha) = match config.mode {
            SpiMode::Mode0 => (0, 0),
            SpiMode::Mode1 => (0, CTRLR0_SCPH),
            SpiMode::Mode2 => (CTRLR0_SCPOL, 0),
            SpiMode::Mode3 => (CTRLR0_SCPOL, CTRLR0_SCPH),
        };

        let ctrlr0 = dfs | cpol | cpha | CTRLR0_TMOD_TR;
        Self::write(CTRLR0, ctrlr0);

        // Baud rate divisor (must be even, minimum 2)
        let mut div = SPI_REF_CLOCK / config.clock_hz;
        if div < 2 {
            div = 2;
        }
        div = (div + 1) & !1; // round up to even
        Self::write(BAUDR, div);

        // Disable all interrupts (polling mode)
        Self::write(IMR, 0);

        self.enable();
        Ok(())
    }

    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<usize, SpiError> {
        let len = tx.len().min(rx.len());
        if len == 0 {
            return Ok(0);
        }

        self.wait_idle();
        self.flush_fifos();
        self.enable();

        // Assert CS
        Self::write(SER, 1);

        let mut tx_idx = 0;
        let mut rx_idx = 0;

        while rx_idx < len {
            // Fill TX FIFO
            while tx_idx < len && (Self::read(SR) & SR_TFNF) != 0 {
                Self::write(DR, tx[tx_idx] as u32);
                tx_idx += 1;
            }

            // Drain RX FIFO
            while rx_idx < len && (Self::read(SR) & SR_RFNE) != 0 {
                rx[rx_idx] = Self::read(DR) as u8;
                rx_idx += 1;
            }
        }

        self.wait_idle();

        // Deassert CS
        Self::write(SER, 0);

        Ok(len)
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, SpiError> {
        if data.is_empty() {
            return Ok(0);
        }

        self.wait_idle();
        self.flush_fifos();
        self.enable();

        Self::write(SER, 1);

        let mut tx_idx = 0;
        let mut rx_discard = 0;

        while rx_discard < data.len() {
            while tx_idx < data.len() && (Self::read(SR) & SR_TFNF) != 0 {
                Self::write(DR, data[tx_idx] as u32);
                tx_idx += 1;
            }
            while rx_discard < tx_idx && (Self::read(SR) & SR_RFNE) != 0 {
                let _ = Self::read(DR);
                rx_discard += 1;
            }
        }

        self.wait_idle();
        Self::write(SER, 0);

        Ok(data.len())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, SpiError> {
        if buf.is_empty() {
            return Ok(0);
        }

        self.wait_idle();
        self.flush_fifos();
        self.enable();

        Self::write(SER, 1);

        let mut tx_idx = 0;
        let mut rx_idx = 0;

        while rx_idx < buf.len() {
            // Send dummy bytes to clock data in
            while tx_idx < buf.len() && (Self::read(SR) & SR_TFNF) != 0 {
                Self::write(DR, 0x00);
                tx_idx += 1;
            }
            while rx_idx < tx_idx && (Self::read(SR) & SR_RFNE) != 0 {
                buf[rx_idx] = Self::read(DR) as u8;
                rx_idx += 1;
            }
        }

        self.wait_idle();
        Self::write(SER, 0);

        Ok(buf.len())
    }
}
