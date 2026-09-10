core::arch::global_asm!(include_str!("boot.S"));
core::arch::global_asm!(include_str!("vectors.S"));

pub mod exceptions;
pub mod gic;
pub mod timer;
