/// HAL trait for symmetric multiprocessing boot.
pub trait SmpBoot {
    /// Return the current core's ID (0-based).
    fn core_id() -> usize;

    /// Number of cores available on the platform.
    fn num_cores() -> usize;

    /// Wake a secondary core. The core will jump to the provided entry
    /// function with `core_id` as its argument. Must be called from the
    /// primary core (core 0) after all shared state is initialized.
    fn start_core(core_id: usize, entry: extern "C" fn(usize) -> !);
}
