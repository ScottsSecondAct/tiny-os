// SPDX-License-Identifier: MIT
// AArch64 bare-metal boot stub for tiny_os.
//
// Entry point: _start (placed in .text.boot so the linker script puts it
// first at 0x80000, exactly where the RPi firmware jumps).
//
// Sequence:
//   1. Park all secondary cores (MPIDR_EL1.Aff0 != 0) in a WFE loop.
//   2. If we entered at EL2, configure HCR_EL2 and eret to EL1h.
//   3. Set the stack pointer to __stack_top.
//   4. Zero the .bss section (16-byte aligned stp xzr,xzr stores).
//   5. Branch to kmain. If kmain ever returns, spin forever.

    .section ".text.boot", "ax"
    .globl _start
    .type  _start, %function
_start:
    // ---------------------------------------------------------------
    // 1. Park secondary cores.
    //    MPIDR_EL1[7:0] = Aff0 = CPU index on a single-cluster board.
    //    Core 0 continues; cores 1-3 spin on WFE.
    // ---------------------------------------------------------------
    mrs     x0, mpidr_el1
    and     x0, x0, #0xFF
    cbz     x0, .Lprimary       // core 0 → continue

.Lsecondary_park:
    wfe
    b       .Lsecondary_park

.Lprimary:
    // ---------------------------------------------------------------
    // 2. Detect current Exception Level.
    //    CurrentEL[3:2]: 0b10 = EL2, 0b01 = EL1.
    //    If we're already in EL1, skip the EL2 → EL1 transition.
    // ---------------------------------------------------------------
    mrs     x0, CurrentEL
    lsr     x0, x0, #2
    and     x0, x0, #3
    cmp     x0, #2
    bne     .Lel1_setup         // not EL2 → already at EL1 (or EL3, ignore)

    // ---------------------------------------------------------------
    // 2a. We're in EL2. Configure HCR_EL2 and eret to EL1h.
    //
    //  HCR_EL2.RW  (bit 31) = 1 → EL1 runs AArch64
    //  HCR_EL2.HCD (bit  8) = 1 → disable HVC (optional, keeps it clean)
    // ---------------------------------------------------------------
    mov     x0, #(1 << 31)      // RW=1: EL1 is AArch64
    orr     x0, x0, #(1 << 8)  // HCD=1: disable HVC
    msr     hcr_el2, x0

    // SPSR_EL2: DAIF=1111 (all interrupts masked), M=0b0101 (EL1h)
    // Encoding: bits [9:6]=DAIF=0b1111, bits [3:0]=M=0b0101 → 0x3C5
    mov     x0, #0x3C5
    msr     spsr_el2, x0

    // ELR_EL2: return address = .Lel1_setup
    adr     x0, .Lel1_setup
    msr     elr_el2, x0

    // Ensure EL2 system register writes are visible before eret
    isb
    eret

.Lel1_setup:
    // ---------------------------------------------------------------
    // 3. Set up the stack pointer.
    //    __stack_top is exported by the linker script and is 16-byte
    //    aligned. SP_EL1 is selected automatically when in EL1h (M=5).
    // ---------------------------------------------------------------
    ldr     x0, =__stack_top
    mov     sp, x0

    // ---------------------------------------------------------------
    // 4. Zero the .bss section.
    //    __bss_start and __bss_end are exported by the linker script.
    //    BSS is 16-byte aligned, so we can use stp xzr, xzr safely.
    // ---------------------------------------------------------------
    ldr     x1, =__bss_start
    ldr     x2, =__bss_end

.Lbss_zero:
    cmp     x1, x2
    bge     .Lbss_done
    stp     xzr, xzr, [x1], #16
    b       .Lbss_zero

.Lbss_done:
    // ---------------------------------------------------------------
    // 5. Jump to kmain. The `bl` saves LR, but kmain is `-> !` so it
    //    must never return. The fallback WFE loop is a safety net.
    // ---------------------------------------------------------------
    bl      kmain

.Lhalt:
    wfe
    b       .Lhalt

    .size _start, . - _start
