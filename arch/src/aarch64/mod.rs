// Embed the AArch64 boot assembly. This defines `_start`, parks secondary
// cores, drops from EL2 to EL1 if needed, zeros .bss, and calls `kmain`.
core::arch::global_asm!(include_str!("boot.S"));
