/// BCM2711 PL011 UART0 base address (QEMU raspi4b, low-peripheral mode).
pub const UART0_BASE: usize = 0xFE20_1000;

/// GIC-400 Distributor base address (BCM2711, same as BCM2712).
pub const GIC_DIST_BASE: usize = 0xFF84_1000;

/// GIC-400 CPU Interface base address.
pub const GIC_CPU_BASE: usize = 0xFF84_2000;
