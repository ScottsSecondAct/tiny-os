/// HAL trait for task context save/restore.
///
/// Implementors provide the architecture-specific mechanism for
/// creating initial task contexts and switching between them.
pub trait Context {
    /// Opaque saved context type.
    type SavedContext;

    /// Create an initial context that will begin executing `entry(arg)` using
    /// the given stack. `stack_top` points one past the last usable byte.
    fn new_context(entry: fn(usize) -> !, arg: usize, stack_top: *mut u8) -> Self::SavedContext;

    /// Switch from the current task to `next`. Saves the current SP into
    /// `*current_sp` and restores SP from `*next_sp`, then returns into
    /// the next task's saved context.
    ///
    /// # Safety
    /// Both pointers must be valid and the stack behind `next_sp` must
    /// contain a context created by `new_context` or a previous switch.
    unsafe fn switch(current_sp: *mut u64, next_sp: *const u64);
}
