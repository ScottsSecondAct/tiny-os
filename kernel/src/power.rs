use arch::aarch64::mailbox;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerError {
    NotAvailable,
    InvalidFrequency,
    MailboxError,
}

pub fn get_cpu_freq() -> Result<u32, PowerError> {
    mailbox::get_clock_rate().ok_or(PowerError::NotAvailable)
}

pub fn set_cpu_freq(hz: u32) -> Result<u32, PowerError> {
    if hz == 0 {
        return Err(PowerError::InvalidFrequency);
    }
    mailbox::set_clock_rate(hz).ok_or(PowerError::MailboxError)
}

pub fn get_max_freq() -> Result<u32, PowerError> {
    mailbox::get_max_clock().ok_or(PowerError::NotAvailable)
}

pub fn get_min_freq() -> Result<u32, PowerError> {
    mailbox::get_min_clock().ok_or(PowerError::NotAvailable)
}

pub fn get_voltage() -> Result<u32, PowerError> {
    mailbox::get_voltage().ok_or(PowerError::NotAvailable)
}

pub fn cpu_idle() {
    // SAFETY: WFI is a safe hint instruction that waits for an interrupt.
    unsafe { core::arch::asm!("wfi"); }
}
