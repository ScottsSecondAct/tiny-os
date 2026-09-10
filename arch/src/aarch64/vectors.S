// SPDX-License-Identifier: MIT
// AArch64 exception vector table for tiny_os.
//
// VBAR_EL1 must be 2 KB (0x800) aligned. Each of the 16 entries is 0x80
// bytes (32 instructions). We branch from each entry to a handler stub
// that saves the full TrapFrame, calls a Rust handler, restores, and ERETs.
//
// TrapFrame layout (272 bytes, matches arch::aarch64::exceptions::TrapFrame):
//   [sp + 0]     x0
//   [sp + 8]     x1
//   ...
//   [sp + 232]   x29
//   [sp + 240]   x30 (LR)
//   [sp + 248]   elr_el1
//   [sp + 256]   spsr_el1
//   [sp + 264]   sp_el0

.macro SAVE_REGS
    sub     sp, sp, #272

    stp     x0,  x1,  [sp, #0]
    stp     x2,  x3,  [sp, #16]
    stp     x4,  x5,  [sp, #32]
    stp     x6,  x7,  [sp, #48]
    stp     x8,  x9,  [sp, #64]
    stp     x10, x11, [sp, #80]
    stp     x12, x13, [sp, #96]
    stp     x14, x15, [sp, #112]
    stp     x16, x17, [sp, #128]
    stp     x18, x19, [sp, #144]
    stp     x20, x21, [sp, #160]
    stp     x22, x23, [sp, #176]
    stp     x24, x25, [sp, #192]
    stp     x26, x27, [sp, #208]
    stp     x28, x29, [sp, #224]
    str     x30,      [sp, #240]

    mrs     x0, elr_el1
    mrs     x1, spsr_el1
    stp     x0, x1, [sp, #248]

    mrs     x0, sp_el0
    str     x0, [sp, #264]
.endm

.macro RESTORE_REGS
    ldr     x0, [sp, #264]
    msr     sp_el0, x0

    ldp     x0, x1, [sp, #248]
    msr     elr_el1, x0
    msr     spsr_el1, x1

    ldr     x30,      [sp, #240]
    ldp     x28, x29, [sp, #224]
    ldp     x26, x27, [sp, #208]
    ldp     x24, x25, [sp, #192]
    ldp     x22, x23, [sp, #176]
    ldp     x20, x21, [sp, #160]
    ldp     x18, x19, [sp, #144]
    ldp     x16, x17, [sp, #128]
    ldp     x14, x15, [sp, #112]
    ldp     x12, x13, [sp, #96]
    ldp     x10, x11, [sp, #80]
    ldp     x8,  x9,  [sp, #64]
    ldp     x6,  x7,  [sp, #48]
    ldp     x4,  x5,  [sp, #32]
    ldp     x2,  x3,  [sp, #16]
    ldp     x0,  x1,  [sp, #0]

    add     sp, sp, #272
    eret
.endm

// ===================================================================
// Vector table — 16 entries × 0x80 bytes each, 0x800-aligned.
// ===================================================================
    .section ".text.vectors", "ax"
    .balign 0x800
    .globl __vectors
__vectors:

// ------- Current EL with SP_EL0 (not used; we run in EL1h) --------
.balign 0x80; b _vec_unhandled     // Synchronous
.balign 0x80; b _vec_unhandled     // IRQ
.balign 0x80; b _vec_unhandled     // FIQ
.balign 0x80; b _vec_unhandled     // SError

// ------- Current EL with SP_ELx (EL1h — this is the kernel) -------
.balign 0x80; b _vec_sync_el1h     // Synchronous (kernel fault)
.balign 0x80; b _vec_irq_el1h      // IRQ (interrupt while in kernel)
.balign 0x80; b _vec_unhandled     // FIQ
.balign 0x80; b _vec_unhandled     // SError

// ------- Lower EL using AArch64 (EL0 tasks, Phase 4+) -------------
.balign 0x80; b _vec_sync_el0      // Synchronous (SVC / fault)
.balign 0x80; b _vec_irq_el0       // IRQ (interrupt while in EL0)
.balign 0x80; b _vec_unhandled     // FIQ
.balign 0x80; b _vec_unhandled     // SError

// ------- Lower EL using AArch32 (not supported) -------------------
.balign 0x80; b _vec_unhandled
.balign 0x80; b _vec_unhandled
.balign 0x80; b _vec_unhandled
.balign 0x80; b _vec_unhandled

// ===================================================================
// Handler stubs
// ===================================================================

_vec_sync_el1h:
    SAVE_REGS
    mov     x0, sp              // &TrapFrame
    mov     x1, #0              // source = EL1
    bl      handle_sync
    RESTORE_REGS

_vec_irq_el1h:
    SAVE_REGS
    mov     x0, sp              // &TrapFrame
    bl      handle_irq
    RESTORE_REGS

_vec_sync_el0:
    SAVE_REGS
    mov     x0, sp
    mov     x1, #1              // source = EL0
    bl      handle_sync
    RESTORE_REGS

_vec_irq_el0:
    SAVE_REGS
    mov     x0, sp
    bl      handle_irq
    RESTORE_REGS

_vec_unhandled:
    SAVE_REGS
    mov     x0, sp
    bl      handle_unhandled
    RESTORE_REGS
