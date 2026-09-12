pub trait UserContext {
    fn new_user_context(entry: usize, user_sp: usize, arg: usize, kernel_stack_top: *mut u8)
        -> u64;
}
