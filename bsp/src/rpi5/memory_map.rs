// RP1 southbridge peripheral window as seen from the BCM2712 CPU.
//
// Physical layout:
//   BCM2712 maps the RP1 PCIe BAR at 0x0001_F000_0000 (36-bit address).
//   RP1 internal peripherals start at RP1-local address 0x4000_0000.
//
// UART0 derivation:
//   RP1-local UART0 base = 0x4006_C000
//   Offset from RP1 window = 0x4006_C000 - 0x4000_0000 = 0x6_C000
//   CPU physical address   = 0x1F_0000_0000 + 0x6_C000 = 0x1F_0006_C000

/// Base address of the RP1 peripheral window (CPU physical address).
pub const RP1_BASE: usize = 0x0001_F000_0000;

/// Physical address of RP1 UART0 (PL011-compatible).
pub const RP1_UART0_BASE: usize = RP1_BASE + 0x0006_C000;

/// GIC-400 Distributor base address.
pub const GIC_DIST_BASE: usize = 0xFF84_1000;

/// GIC-400 CPU Interface base address.
pub const GIC_CPU_BASE: usize = 0xFF84_2000;

/// Default RAM region (used when DTB parsing fails).
pub const RAM_BASE: usize = 0;
pub const RAM_SIZE_DEFAULT: usize = 0x1_0000_0000; // 4 GB

/// BCM2712 peripheral MMIO region (covers GPIO, GIC, etc.).
pub const PERIPH_BASE: usize = 0xFE00_0000;
pub const PERIPH_SIZE: usize = 0x0200_0000; // 32 MB

/// RP1 southbridge MMIO region (UART, SPI, I²C, Ethernet).
pub const RP1_PERIPH_BASE: usize = 0x1F_0000_0000;
pub const RP1_PERIPH_SIZE: usize = 0x0040_0000; // 4 MB
