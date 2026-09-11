/// BCM2711 PL011 UART0 base address (QEMU raspi4b, low-peripheral mode).
pub const UART0_BASE: usize = 0xFE20_1000;

/// GIC-400 Distributor base address (BCM2711, same as BCM2712).
pub const GIC_DIST_BASE: usize = 0xFF84_1000;

/// GIC-400 CPU Interface base address.
pub const GIC_CPU_BASE: usize = 0xFF84_2000;

/// Default RAM region (used when DTB parsing fails).
pub const RAM_BASE: usize = 0;
pub const RAM_SIZE_DEFAULT: usize = 0x4000_0000; // 1 GB

/// BCM2711 EMMC2 (Arasan SDHCI) base address.
pub const EMMC2_BASE: usize = 0xFE34_0000;

/// BCM2711 peripheral MMIO region (covers UART, GPIO, GIC, etc.).
pub const PERIPH_BASE: usize = 0xFE00_0000;
pub const PERIPH_SIZE: usize = 0x0200_0000; // 32 MB
