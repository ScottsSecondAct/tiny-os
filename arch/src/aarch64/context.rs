use crate::context::Context;

extern "C" {
    fn context_switch(current_sp: *mut u64, next_sp: *const u64);
    fn task_trampoline();
    fn task_trampoline_user();
}

pub struct Aarch64Context;

impl Context for Aarch64Context {
    type SavedContext = u64;

    fn new_context(entry: fn(usize) -> !, arg: usize, stack_top: *mut u8) -> u64 {
        // Build a fake saved-register frame on the task's stack so the first
        // context_switch into this task pops the frame and RETs into
        // task_trampoline, which then calls entry(arg).
        //
        // Layout (96 bytes, matching context_switch.S):
        //   [sp + 0]  x19 = entry
        //   [sp + 8]  x20 = arg
        //   [sp + 16] x21 = 0
        //   ...
        //   [sp + 80] x29 = 0
        //   [sp + 88] x30 = task_trampoline address
        let sp = stack_top as usize;
        let frame_sp = sp - 96;
        let frame = frame_sp as *mut u64;

        unsafe {
            // Zero the entire frame first.
            core::ptr::write_bytes(frame, 0, 12);
            // x19 = entry function pointer
            frame.add(0).write(entry as usize as u64);
            // x20 = argument
            frame.add(1).write(arg as u64);
            // x30 (LR) = trampoline address
            frame
                .add(11)
                .write(task_trampoline as *const () as usize as u64);
        }

        frame_sp as u64
    }

    unsafe fn switch(current_sp: *mut u64, next_sp: *const u64) {
        context_switch(current_sp, next_sp);
    }
}

impl Aarch64Context {
    pub fn new_user_context(
        entry: usize,
        user_sp: usize,
        arg: usize,
        kernel_stack_top: *mut u8,
    ) -> u64 {
        let sp = kernel_stack_top as usize;
        let frame_sp = sp - 96;
        let frame = frame_sp as *mut u64;

        unsafe {
            core::ptr::write_bytes(frame, 0, 12);
            frame.add(0).write(entry as u64); // x19 = EL0 entry
            frame.add(1).write(arg as u64); // x20 = arg
            frame.add(2).write(user_sp as u64); // x21 = user SP
            frame
                .add(11)
                .write(task_trampoline_user as *const () as usize as u64);
        }

        frame_sp as u64
    }
}
