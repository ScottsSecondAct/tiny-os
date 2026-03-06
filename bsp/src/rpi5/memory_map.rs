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
