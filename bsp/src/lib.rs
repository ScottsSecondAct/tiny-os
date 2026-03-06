#![no_std]

#[cfg(feature = "bsp-rpi5")]
pub mod rpi5;
#[cfg(feature = "bsp-qemu")]
pub mod qemu_virt;

// Re-export the active platform UART as `PlatformUart` so kernel code
// is written against a single name regardless of which BSP is compiled.
#[cfg(feature = "bsp-rpi5")]
pub use rpi5::Rp1Uart as PlatformUart;

#[cfg(feature = "bsp-qemu")]
pub use qemu_virt::Pl011Uart as PlatformUart;
