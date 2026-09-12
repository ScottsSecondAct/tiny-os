#![no_std]
#![allow(dead_code)]
#![allow(clippy::new_without_default)]

#[cfg(feature = "bsp-qemu")]
pub mod qemu_virt;
#[cfg(feature = "bsp-rpi5")]
pub mod rpi5;

#[cfg(feature = "bsp-rpi5")]
pub use rpi5::Rp1Uart as PlatformUart;

#[cfg(feature = "bsp-qemu")]
pub use qemu_virt::Pl011Uart as PlatformUart;

#[cfg(feature = "bsp-rpi5")]
pub use rpi5::memory_map::{EMMC2_BASE, GIC_CPU_BASE, GIC_DIST_BASE, MAILBOX_BASE};

#[cfg(feature = "bsp-qemu")]
pub use qemu_virt::memory_map::{EMMC2_BASE, GIC_CPU_BASE, GIC_DIST_BASE, MAILBOX_BASE};

#[cfg(feature = "bsp-rpi5")]
pub use rpi5::{Rp1Eth, Rp1Gpio, Rp1I2c, Rp1Pwm, Rp1Serial, Rp1Spi, Rp1Usb};
