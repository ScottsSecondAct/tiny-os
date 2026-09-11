core::arch::global_asm!(include_str!("boot.S"));
core::arch::global_asm!(include_str!("vectors.S"));
core::arch::global_asm!(include_str!("context_switch.S"));

pub mod context;
pub mod exceptions;
pub mod gic;
pub mod mmu;
pub mod smp;
pub mod timer;
