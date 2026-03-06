// RP1 UART0 driver — PL011-compatible.
//
// The RP1 UART register map (offsets from RP1_UART0_BASE):
//   0x000  DR      Data Register
//   0x018  FR      Flag Register      [bit 5 = TXFF, bit 4 = RXFE, bit 3 = BUSY]
//   0x024  IBRD    Integer Baud Rate Divisor
//   0x028  FBRD    Fractional Baud Rate Divisor
//   0x02C  LCR_H   Line Control Register
//   0x030  CR      Control Register
//
// Baud rate divisor for 115200 @ 48 MHz reference clock:
//   Divisor = 48_000_000 / (16 * 115200) = 26.042...
//   IBRD = 26, FBRD = round(0.042 * 64) = 3

use super::memory_map::RP1_UART0_BASE;
use arch::uart::UartDriver;
use core::fmt;

// Register offsets
const DR:    usize = 0x000;
const FR:    usize = 0x018;
const IBRD:  usize = 0x024;
const FBRD:  usize = 0x028;
const LCR_H: usize = 0x02C;
const CR:    usize = 0x030;

// Flag Register bits
const FR_TXFF: u32 = 1 << 5; // Transmit FIFO Full
const FR_BUSY: u32 = 1 << 3; // UART Busy

// LCR_H bits
const LCR_H_FEN:  u32 = 1 << 4; // Enable FIFOs
const LCR_H_WLEN: u32 = 0b11 << 5; // Word length = 8 bits (11)

// CR bits
const CR_UARTEN: u32 = 1 << 0; // UART Enable
const CR_TXE:    u32 = 1 << 8; // Transmit Enable
const CR_RXE:    u32 = 1 << 9; // Receive Enable

/// Zero-size type representing the RP1 UART0 peripheral.
/// Safe to construct: MMIO access is gated behind `init()`.
pub struct Rp1Uart;

impl Rp1Uart {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (RP1_UART0_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read(offset: usize) -> u32 {
        // SAFETY: RP1_UART0_BASE is a valid MMIO address on Pi 5 when
        // pciex4_reset=0 and uart_early_init=1 are set in config.txt.
        // Volatile read prevents the compiler from reordering or eliding.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read — valid MMIO, volatile prevents reordering.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl UartDriver for Rp1Uart {
    fn init(&mut self) {
        // 1. Disable UART.
        Self::write(CR, 0);

        // 2. Wait for any in-progress transmission to complete.
        while Self::read(FR) & FR_BUSY != 0 {}

        // 3. Flush FIFO by clearing LCR_H.FEN, then set baud divisors.
        Self::write(LCR_H, 0);
        Self::write(IBRD, 26);
        Self::write(FBRD, 3);

        // 4. Enable FIFO, set 8N1 (WLEN=0b11, no parity, 1 stop bit).
        Self::write(LCR_H, LCR_H_FEN | LCR_H_WLEN);

        // 5. Re-enable UART with TX and RX.
        Self::write(CR, CR_UARTEN | CR_TXE | CR_RXE);
    }

    fn write_byte(&mut self, byte: u8) {
        // Poll until TX FIFO has space.
        while Self::read(FR) & FR_TXFF != 0 {}
        Self::write(DR, byte as u32);
    }
}

impl fmt::Write for Rp1Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            // Translate bare LF to CR+LF for serial terminals.
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}
