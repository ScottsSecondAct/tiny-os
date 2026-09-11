// AArch64 context switch for tiny_os.
//
// Saves callee-saved registers (x19-x30) and SP of the current task,
// then restores the next task's registers and SP. Caller-saved registers
// (x0-x18) are handled by the C calling convention.
//
// Saved context layout on the task's kernel stack (14 × 8 = 112 bytes):
//   [sp + 0]    x19
//   [sp + 8]    x20
//   [sp + 16]   x21
//   [sp + 24]   x22
//   [sp + 32]   x23
//   [sp + 40]   x24
//   [sp + 48]   x25
//   [sp + 56]   x26
//   [sp + 64]   x27
//   [sp + 72]   x28
//   [sp + 80]   x29 (frame pointer)
//   [sp + 88]   x30 (link register — return address)
//
// void context_switch(u64 *current_sp, u64 *next_sp)
//   x0 = pointer to current task's saved SP slot
//   x1 = pointer to next task's saved SP slot

    .section .text
    .globl context_switch
    .type context_switch, @function
context_switch:
    // Save callee-saved registers on current stack.
    stp     x19, x20, [sp, #-96]!
    stp     x21, x22, [sp, #16]
    stp     x23, x24, [sp, #32]
    stp     x25, x26, [sp, #48]
    stp     x27, x28, [sp, #64]
    stp     x29, x30, [sp, #80]

    // Save current SP.
    mov     x9, sp
    str     x9, [x0]

    // Load next task's SP.
    ldr     x9, [x1]
    mov     sp, x9

    // Restore callee-saved registers from next stack.
    ldp     x29, x30, [sp, #80]
    ldp     x27, x28, [sp, #64]
    ldp     x25, x26, [sp, #48]
    ldp     x23, x24, [sp, #32]
    ldp     x21, x22, [sp, #16]
    ldp     x19, x20, [sp], #96

    ret


// Task entry trampoline. New tasks start here after their first context switch.
// x19 = entry function pointer, x20 = argument.
// The scheduler lock is held when we get here (by the schedule() or start()
// that switched to this task), so release it before enabling IRQs.
    .globl task_trampoline
    .type task_trampoline, @function
task_trampoline:
    bl      sched_unlock_new_task
    msr     daifclr, #2         // enable IRQs before entering the task
    mov     x0, x20             // arg
    blr     x19                 // entry(arg) -> !
    // entry is divergent (-> !), but if it returns, halt.
    b       .
