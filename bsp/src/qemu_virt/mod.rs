// QEMU raspi3b UART driver — BCM2837 PL011 UART0.
//
// QEMU `-M raspi3b` maps the BCM2837 PL011 UART0 at 0x3F20_1000.
// QEMU pre-initializes this UART at 115200 baud, so `init()` is a no-op.
//
// Register offsets (PL011, same layout as RP1):
//   0x000  DR   Data Register
//   0x018  FR   Flag Register  [bit 5 = TXFF]

use arch::uart::UartDriver;
use core::fmt;

const UART0_BASE: usize = 0x3F20_1000;

const DR: usize = 0x000;
const FR: usize = 0x018;

const FR_TXFF: u32 = 1 << 5;

/// Zero-size type representing the BCM2837 QEMU PL011 UART0.
pub struct Pl011Uart;

impl Pl011Uart {
    pub const fn new() -> Self {
        Self
    }

    #[inline(always)]
    fn reg(offset: usize) -> *mut u32 {
        (UART0_BASE + offset) as *mut u32
    }

    #[inline(always)]
    fn read(offset: usize) -> u32 {
        // SAFETY: UART0_BASE is the BCM2837 PL011 UART0 MMIO address,
        // which is valid under QEMU `-M raspi3b`. Volatile prevents
        // the compiler from eliding or reordering register accesses.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same invariant as read.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl UartDriver for Pl011Uart {
    fn init(&mut self) {
        // QEMU initializes the PL011 automatically; nothing to do here.
    }

    fn write_byte(&mut self, byte: u8) {
        while Self::read(FR) & FR_TXFF != 0 {}
        Self::write(DR, byte as u32);
    }
}

impl fmt::Write for Pl011Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}
