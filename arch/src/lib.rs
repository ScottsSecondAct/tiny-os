#![no_std]
#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]

pub mod aarch64;
pub mod block;
pub mod context;
pub mod crypto_engine;
pub mod dma;
pub mod gpio;
pub mod i2c;
pub mod irq;
pub mod mm;
pub mod net;
pub mod pwm;
pub mod rtc;
pub mod serial;
pub mod smp;
pub mod spi;
pub mod timer;
pub mod uart;
pub mod usb;
pub mod user;
