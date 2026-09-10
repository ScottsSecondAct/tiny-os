pub trait InterruptController {
    fn init(&mut self);
    fn enable(&mut self, irq_id: u32);
    fn disable(&mut self, irq_id: u32);
    fn acknowledge(&mut self) -> u32;
    fn end_of_interrupt(&mut self, irq_id: u32);
    fn set_priority(&mut self, irq_id: u32, priority: u8);
}
