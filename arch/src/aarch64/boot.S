// SPDX-License-Identifier: MIT
// AArch64 bare-metal boot stub for tiny_os.
//
// Entry point: _start (placed in .text.boot so the linker script puts it
// first at 0x80000, exactly where the RPi firmware jumps).
//
// Sequence:
//   1. Park all secondary cores (MPIDR_EL1.Aff0 != 0) in a WFE loop.
//   2. Drop from EL3→EL1 (secure) or EL2→EL1 depending on entry EL.
//   3. Set the stack pointer to __stack_top.
//   4. Zero the .bss section (16-byte aligned stp xzr,xzr stores).
//   5. Enable FP/SIMD, install exception vector table (VBAR_EL1).
//   6. Branch to kmain. If kmain ever returns, spin forever.

    .section ".text.boot", "ax"
    .globl _start
    .type  _start, %function
_start:
    // ---------------------------------------------------------------
    // 0. Preserve DTB pointer (firmware passes it in x0).
    // ---------------------------------------------------------------
    mov     x19, x0

    // ---------------------------------------------------------------
    // 1. Park secondary cores.
    // ---------------------------------------------------------------
    mrs     x0, mpidr_el1
    and     x0, x0, #0xFF
    cbz     x0, .Lprimary

.Lsecondary_park:
    wfe
    b       .Lsecondary_park

.Lprimary:
    // ---------------------------------------------------------------
    // 2. Detect current Exception Level and drop to EL1.
    // ---------------------------------------------------------------
    mrs     x0, CurrentEL
    lsr     x0, x0, #2
    and     x0, x0, #3
    cmp     x0, #3
    beq     .Lel3_setup
    cmp     x0, #2
    beq     .Lel2_setup
    b       .Lel1_setup

    // ---------------------------------------------------------------
    // EL3 → EL1 (secure). QEMU raspi4b starts at EL3.
    //
    // Stay in secure state (NS=0) so the GIC Group 0 interrupts are
    // delivered as IRQ. QEMU's GIC-400 emulation doesn't reliably
    // handle IGROUPR writes for PPIs, so we keep everything Group 0
    // and access the GIC via the secure aliases.
    // ---------------------------------------------------------------
.Lel3_setup:
    // Configure EL2 registers from EL3 so EL1 is AArch64 and timers
    // are accessible, even though we skip executing at EL2.
    mov     x0, #(1 << 31)      // HCR_EL2.RW=1: EL1 is AArch64
    msr     hcr_el2, x0

    mov     x0, #3
    msr     cnthctl_el2, x0     // EL1 can access physical timer/counter
    msr     cntvoff_el2, xzr    // No virtual counter offset

    msr     cptr_el2, xzr       // Don't trap FP/SIMD at EL2
    msr     cptr_el3, xzr       // Don't trap FP/SIMD at EL3

    // SCR_EL3: RW=1 (lower ELs AArch64), NS=0 (secure), ST=1.
    // NS=0 keeps lower ELs in secure state so the GIC secure aliases
    // are used — Group 0 interrupts are acknowledged via GICC_IAR.
    mov     x0, #(1 << 10)      // RW
    orr     x0, x0, #(1 << 4)   // ST (secure timer access for EL1)
    msr     scr_el3, x0

    // SPSR_EL3: DAIF=1111, M=EL1h (0b00101) → 0x3C5
    mov     x0, #0x3C5
    msr     spsr_el3, x0

    adr     x0, .Lel1_setup
    msr     elr_el3, x0
    isb
    eret

    // ---------------------------------------------------------------
    // EL2 → EL1. Real Pi 5 firmware enters here.
    // ---------------------------------------------------------------
.Lel2_setup:
    mov     x0, #(1 << 31)      // HCR_EL2.RW=1: EL1 is AArch64
    msr     hcr_el2, x0

    mov     x0, #3
    msr     cnthctl_el2, x0     // EL1 can access CNTP_* and CNTPCT
    msr     cntvoff_el2, xzr
    msr     cptr_el2, xzr       // Don't trap FP/SIMD at EL2

    // SPSR_EL2: DAIF=1111, M=EL1h → 0x3C5
    mov     x0, #0x3C5
    msr     spsr_el2, x0

    adr     x0, .Lel1_setup
    msr     elr_el2, x0
    isb
    eret

    // ---------------------------------------------------------------
    // EL1 common setup.
    // ---------------------------------------------------------------
.Lel1_setup:
    ldr     x0, =__stack_top
    mov     sp, x0

    // Zero .bss
    ldr     x1, =__bss_start
    ldr     x2, =__bss_end
.Lbss_zero:
    cmp     x1, x2
    bge     .Lbss_done
    stp     xzr, xzr, [x1], #16
    b       .Lbss_zero
.Lbss_done:

    // Store DTB pointer (preserved in x19 since _start).
    ldr     x0, =DTB_PTR
    str     x19, [x0]

    // Enable FP/SIMD at EL1 (CPACR_EL1.FPEN = 0b11).
    mov     x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    // Install exception vector table.
    ldr     x0, =__vectors
    msr     vbar_el1, x0
    isb

    bl      kmain

.Lhalt:
    wfe
    b       .Lhalt

    .size _start, . - _start
