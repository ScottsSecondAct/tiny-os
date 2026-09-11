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

/// BCM2712 VideoCore mailbox base address.
pub const MAILBOX_BASE: usize = 0xFE00_B880;

/// BCM2712 EMMC2 (Arasan SDHCI) base address.
pub const EMMC2_BASE: usize = 0xFE34_0000;

/// BCM2712 peripheral MMIO region (covers GPIO, GIC, etc.).
pub const PERIPH_BASE: usize = 0xFE00_0000;
pub const PERIPH_SIZE: usize = 0x0200_0000; // 32 MB

/// RP1 SPI0 base (DW_apb_ssi, RP1-local 0x4005_0000).
pub const RP1_SPI0_BASE: usize = RP1_BASE + 0x0005_0000;

/// RP1 I2C0 base (DW_apb_i2c, RP1-local 0x4007_0000).
pub const RP1_I2C0_BASE: usize = RP1_BASE + 0x0007_0000;

/// RP1 GPIO base (RP1-local 0x400D_0000).
pub const RP1_GPIO_BASE: usize = RP1_BASE + 0x000D_0000;

/// RP1 PWM0 base (RP1-local 0x4009_8000).
pub const RP1_PWM_BASE: usize = RP1_BASE + 0x0009_8000;

/// RP1 UART1 base (PL011, RP1-local 0x4006_C800).
pub const RP1_UART1_BASE: usize = RP1_BASE + 0x0006_C800;

/// RP1 Ethernet MAC base (Synopsys GMAC, RP1-local 0x4010_0000).
pub const RP1_ETH_BASE: usize = RP1_BASE + 0x0010_0000;

/// RP1 USB host controller base (xHCI, RP1-local 0x4020_0000).
pub const RP1_USB_BASE: usize = RP1_BASE + 0x0020_0000;

/// RP1 southbridge MMIO region (UART, SPI, I²C, Ethernet).
pub const RP1_PERIPH_BASE: usize = 0x1F_0000_0000;
pub const RP1_PERIPH_SIZE: usize = 0x0040_0000; // 4 MB
