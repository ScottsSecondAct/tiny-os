// QEMU raspi4b UART driver — BCM2711 PL011 UART0.
//
// QEMU `-M raspi4b` maps the BCM2711 PL011 UART0 at 0xFE20_1000.
// QEMU pre-initializes this UART at 115200 baud, so `init()` is a no-op.

use super::memory_map::UART0_BASE;
use arch::uart::UartDriver;
use core::fmt;

const DR: usize = 0x000;
const FR: usize = 0x018;
const LCRH: usize = 0x02C;

const FR_TXFF: u32 = 1 << 5;

pub struct Pl011Uart;

impl Default for Pl011Uart {
    fn default() -> Self {
        Self::new()
    }
}

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
        // SAFETY: UART0_BASE is the BCM2711 PL011 UART0 MMIO address,
        // valid under QEMU `-M raspi4b`. Volatile prevents reordering.
        unsafe { core::ptr::read_volatile(Self::reg(offset)) }
    }

    #[inline(always)]
    fn write(offset: usize, val: u32) {
        // SAFETY: Same as read.
        unsafe { core::ptr::write_volatile(Self::reg(offset), val) }
    }
}

impl UartDriver for Pl011Uart {
    fn init(&mut self) {
        let lcrh = Self::read(LCRH);
        Self::write(LCRH, lcrh | (1 << 4));
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
