#![no_std]

#[cfg(feature = "bsp-qemu")]
pub mod qemu_virt;
#[cfg(feature = "bsp-rpi5")]
pub mod rpi5;

#[cfg(feature = "bsp-rpi5")]
pub use rpi5::Rp1Uart as PlatformUart;

#[cfg(feature = "bsp-qemu")]
pub use qemu_virt::Pl011Uart as PlatformUart;

#[cfg(feature = "bsp-rpi5")]
pub use rpi5::memory_map::{EMMC2_BASE, GIC_CPU_BASE, GIC_DIST_BASE};

#[cfg(feature = "bsp-qemu")]
pub use qemu_virt::memory_map::{EMMC2_BASE, GIC_CPU_BASE, GIC_DIST_BASE};
