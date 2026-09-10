use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

#[repr(C)]
pub struct TrapFrame {
    pub regs: [u64; 31], // x0-x30
    pub elr_el1: u64,
    pub spsr_el1: u64,
    pub sp_el0: u64,
}

static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

type IrqHandler = fn();

const MAX_IRQS: usize = 64;

struct IrqTable(UnsafeCell<[Option<IrqHandler>; MAX_IRQS]>);

// SAFETY: The table is written only during init (before interrupts are
// enabled) and read only from IRQ context on this single core.
unsafe impl Sync for IrqTable {}

static IRQ_TABLE: IrqTable = IrqTable(UnsafeCell::new([None; MAX_IRQS]));

/// Register an IRQ handler. Must be called before interrupts are enabled.
pub fn register_irq(irq_id: u32, handler: IrqHandler) {
    let id = irq_id as usize;
    assert!(id < MAX_IRQS, "IRQ id out of range");
    // SAFETY: Called during init before interrupts are enabled.
    unsafe {
        (*IRQ_TABLE.0.get())[id] = Some(handler);
    }
}

pub fn tick_count() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed)
}

pub(crate) fn increment_tick() {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Look up and invoke the handler for the given IRQ. Returns true if handled.
pub fn dispatch_irq(irq_id: u32) -> bool {
    let id = irq_id as usize;
    // SAFETY: Table is only written during init before interrupts are enabled.
    let handler = unsafe { (*IRQ_TABLE.0.get()).get(id).copied().flatten() };
    if let Some(h) = handler {
        h();
        true
    } else {
        false
    }
}

/// Read ESR_EL1 (Exception Syndrome Register).
#[inline]
pub fn read_esr() -> u64 {
    let esr: u64;
    unsafe { core::arch::asm!("mrs {}, esr_el1", out(reg) esr) };
    esr
}
