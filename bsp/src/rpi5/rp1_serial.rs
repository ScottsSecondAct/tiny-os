// RP1 user-facing UART driver — PL011-compatible ports 1-5.
//
// RP1 contains 6 PL011 UARTs (UART0-5). UART0 is reserved for the kernel
// console. This driver exposes ports 1-5 for user-space serial I/O.
//
// Register offsets (per port, from port base address):
//   0x000  DR      Data Register (TX/RX FIFO)
//   0x018  FR      Flag Register (TXFF, RXFE, BUSY)
//   0x024  IBRD    Integer Baud Rate Divisor
//   0x028  FBRD    Fractional Baud Rate Divisor
//   0x02C  LCR_H   Line Control Register (WLEN, FEN, STP2, PEN, EPS)
//   0x030  CR      Control Register (UARTEN, TXE, RXE, CTSEn, RTSEn)

use super::memory_map::RP1_BASE;
use arch::serial::{FlowControl, Parity, SerialConfig, SerialError, SerialPort, StopBits};

const DR: usize = 0x000;
const FR: usize = 0x018;
const IBRD: usize = 0x024;
const FBRD: usize = 0x028;
const LCR_H: usize = 0x02C;
const CR: usize = 0x030;

// Flag Register bits
const FR_RXFE: u32 = 1 << 4; // RX FIFO Empty
const FR_TXFF: u32 = 1 << 5; // TX FIFO Full
const FR_BUSY: u32 = 1 << 3;

// LCR_H bits
const LCR_H_FEN: u32 = 1 << 4;  // FIFO Enable
const LCR_H_WLEN_8: u32 = 3 << 5; // 8-bit word length
const LCR_H_STP2: u32 = 1 << 3; // Two stop bits
const LCR_H_PEN: u32 = 1 << 1;  // Parity enable
const LCR_H_EPS: u32 = 1 << 2;  // Even parity select

// CR bits
const CR_UARTEN: u32 = 1 << 0;
const CR_TXE: u32 = 1 << 8;
const CR_RXE: u32 = 1 << 9;
const CR_CTSEN: u32 = 1 << 14;
const CR_RTSEN: u32 = 1 << 15;

// RP1 UART reference clock (48 MHz)
const UART_REF_CLOCK: u32 = 48_000_000;

// UART base addresses: UART1-5 at 0x800 stride from UART0
const UART_BASES: [usize; 5] = [
    RP1_BASE + 0x0006_C800, // UART1
    RP1_BASE + 0x0006_D000, // UART2
    RP1_BASE + 0x0006_D800, // UART3
    RP1_BASE + 0x0006_E000, // UART4
    RP1_BASE + 0x0006_E800, // UART5
];

pub struct Rp1Serial;

impl Rp1Serial {
    pub const fn new() -> Self {
        Self
    }

    fn base(port: u8) -> Option<usize> {
        if port >= 1 && port <= 5 {
            Some(UART_BASES[(port - 1) as usize])
        } else {
            None
        }
    }

    #[inline(always)]
    fn reg_at(base: usize, offset: usize) -> *mut u32 {
        (base + offset) as *mut u32
    }

    #[inline(always)]
    fn read_at(base: usize, offset: usize) -> u32 {
        // SAFETY: base is a valid RP1 UART MMIO address, identity-mapped.
        unsafe { core::ptr::read_volatile(Self::reg_at(base, offset)) }
    }

    #[inline(always)]
    fn write_at(base: usize, offset: usize, val: u32) {
        // SAFETY: Same as read_at — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg_at(base, offset), val) }
    }
}

impl SerialPort for Rp1Serial {
    fn open(&mut self, config: &SerialConfig) -> Result<(), SerialError> {
        let base = Self::base(config.port).ok_or(SerialError::PortNotAvailable)?;
        if config.baud_rate == 0 {
            return Err(SerialError::InvalidConfig);
        }

        // Disable UART
        Self::write_at(base, CR, 0);

        // Wait for not busy
        let mut timeout = 100_000u32;
        while Self::read_at(base, FR) & FR_BUSY != 0 {
            timeout -= 1;
            if timeout == 0 {
                return Err(SerialError::Timeout);
            }
        }

        // Compute baud rate divisors: IBRD = ref_clk / (16 * baud), FBRD = frac * 64
        let divider_x64 = (UART_REF_CLOCK as u64 * 4) / config.baud_rate as u64;
        let ibrd = (divider_x64 >> 6) as u32;
        let fbrd = (divider_x64 & 0x3F) as u32;

        Self::write_at(base, IBRD, ibrd);
        Self::write_at(base, FBRD, fbrd);

        // LCR_H: 8-bit, FIFO enabled, optional parity/stop bits
        let mut lcr = LCR_H_WLEN_8 | LCR_H_FEN;
        match config.parity {
            Parity::None => {}
            Parity::Even => lcr |= LCR_H_PEN | LCR_H_EPS,
            Parity::Odd => lcr |= LCR_H_PEN,
        }
        if config.stop_bits == StopBits::Two {
            lcr |= LCR_H_STP2;
        }
        Self::write_at(base, LCR_H, lcr);

        // CR: enable UART + TX + RX, optional flow control
        let mut cr = CR_UARTEN | CR_TXE | CR_RXE;
        if config.flow_control == FlowControl::RtsCts {
            cr |= CR_CTSEN | CR_RTSEN;
        }
        Self::write_at(base, CR, cr);

        Ok(())
    }

    fn write(&mut self, port: u8, data: &[u8]) -> Result<usize, SerialError> {
        let base = Self::base(port).ok_or(SerialError::PortNotAvailable)?;

        for (i, &byte) in data.iter().enumerate() {
            let mut timeout = 100_000u32;
            while Self::read_at(base, FR) & FR_TXFF != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return if i > 0 { Ok(i) } else { Err(SerialError::Timeout) };
                }
            }
            Self::write_at(base, DR, byte as u32);
        }

        Ok(data.len())
    }

    fn read(&mut self, port: u8, buf: &mut [u8]) -> Result<usize, SerialError> {
        let base = Self::base(port).ok_or(SerialError::PortNotAvailable)?;
        let mut count = 0;

        for slot in buf.iter_mut() {
            if Self::read_at(base, FR) & FR_RXFE != 0 {
                break;
            }
            let dr = Self::read_at(base, DR);
            if dr & 0xF00 != 0 {
                return if count > 0 { Ok(count) } else { Err(SerialError::FramingError) };
            }
            *slot = dr as u8;
            count += 1;
        }

        Ok(count)
    }

    fn close(&mut self, port: u8) -> Result<(), SerialError> {
        let base = Self::base(port).ok_or(SerialError::PortNotAvailable)?;
        Self::write_at(base, CR, 0);
        Ok(())
    }

    fn bytes_available(&self, port: u8) -> Result<usize, SerialError> {
        let base = Self::base(port).ok_or(SerialError::PortNotAvailable)?;
        if Self::read_at(base, FR) & FR_RXFE == 0 {
            Ok(1)
        } else {
            Ok(0)
        }
    }
}
