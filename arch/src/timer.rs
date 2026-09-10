pub trait Timer {
    fn init(&mut self, tick_rate_hz: u32);
    fn acknowledge(&mut self);
    fn read_counter(&self) -> u64;
    fn ticks_per_second(&self) -> u64;
}
