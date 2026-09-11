// GIC-400 (GICv2) driver for tiny_os.
//
// Runs from Secure EL1 on QEMU (all interrupts Group 0, FIQEn=0 so
// Group 0 is delivered as IRQ). On real Pi 5 (EL2 entry, non-secure
// EL1), all interrupts are set to Group 1 via IGROUPR.

use core::cell::UnsafeCell;

use crate::irq::InterruptController;

const GICD_CTLR: usize = 0x000;
const GICD_TYPER: usize = 0x004;
const GICD_IGROUPR: usize = 0x080;
const GICD_ISENABLER: usize = 0x100;
const GICD_ICENABLER: usize = 0x180;
const GICD_IPRIORITYR: usize = 0x400;
const GICD_ITARGETSR: usize = 0x800;
const GICD_ICFGR: usize = 0xC00;
const GICD_SGIR: usize = 0xF00;

const GICC_CTLR: usize = 0x000;
const GICC_PMR: usize = 0x004;
const GICC_IAR: usize = 0x00C;
const GICC_EOIR: usize = 0x010;

pub const SGI_RESCHEDULE: u32 = 0;

pub struct Gic400 {
    gicd_base: usize,
    gicc_base: usize,
}

struct GicCell(UnsafeCell<Option<Gic400>>);
// SAFETY: GIC distributor is written once during init. CPU interface
// registers are per-core banked. Access from IRQ context is serialized
// per-core by IRQ masking.
unsafe impl Sync for GicCell {}

static GIC: GicCell = GicCell(UnsafeCell::new(None));

impl Gic400 {
    pub const fn new(gicd_base: usize, gicc_base: usize) -> Self {
        Self {
            gicd_base,
            gicc_base,
        }
    }

    #[inline(always)]
    fn gicd_read(&self, offset: usize) -> u32 {
        // SAFETY: GICD base is a valid GIC MMIO address set by the BSP.
        unsafe { core::ptr::read_volatile((self.gicd_base + offset) as *const u32) }
    }

    #[inline(always)]
    fn gicd_write(&self, offset: usize, val: u32) {
        // SAFETY: Same as gicd_read.
        unsafe { core::ptr::write_volatile((self.gicd_base + offset) as *mut u32, val) }
    }

    #[inline(always)]
    fn gicc_read(&self, offset: usize) -> u32 {
        // SAFETY: GICC base is a valid GIC CPU interface MMIO address.
        unsafe { core::ptr::read_volatile((self.gicc_base + offset) as *const u32) }
    }

    #[inline(always)]
    fn gicc_write(&self, offset: usize, val: u32) {
        // SAFETY: Same as gicc_read.
        unsafe { core::ptr::write_volatile((self.gicc_base + offset) as *mut u32, val) }
    }
}

impl InterruptController for Gic400 {
    fn init(&mut self) {
        self.gicd_write(GICD_CTLR, 0);

        let typer = self.gicd_read(GICD_TYPER);
        let num_irqs = ((typer & 0x1F) + 1) * 32;

        let mut i: usize = 0;
        while i < num_irqs as usize {
            self.gicd_write(GICD_ICENABLER + (i / 32) * 4, 0xFFFF_FFFF);
            i += 32;
        }

        // IGROUPR=0 (all Group 0). From secure EL1 the GICC secure alias
        // acknowledges Group 0. From non-secure EL1 (real Pi 5 path),
        // boot code must set IGROUPR=0xFFFFFFFF before dropping to EL1.
        i = 0;
        while i < num_irqs as usize {
            self.gicd_write(GICD_IGROUPR + (i / 32) * 4, 0);
            i += 32;
        }

        i = 0;
        while i < num_irqs as usize {
            self.gicd_write(GICD_IPRIORITYR + (i / 4) * 4, 0xA0A0_A0A0);
            i += 4;
        }

        // Route all SPIs to core 0.
        i = 32;
        while i < num_irqs as usize {
            self.gicd_write(GICD_ITARGETSR + (i / 4) * 4, 0x0101_0101);
            i += 4;
        }

        // All SPIs level-triggered.
        i = 32;
        while i < num_irqs as usize {
            self.gicd_write(GICD_ICFGR + (i / 16) * 4, 0);
            i += 16;
        }

        // EnableGrp0 from secure, EnableGrp1 from non-secure.
        self.gicd_write(GICD_CTLR, 1);

        self.gicc_write(GICC_PMR, 0xFF);
        // EnableGrp0=1, FIQEn=0 → Group 0 delivered as IRQ.
        self.gicc_write(GICC_CTLR, 1);
    }

    fn enable(&mut self, irq_id: u32) {
        let reg = (irq_id / 32) as usize;
        let bit = irq_id % 32;
        self.gicd_write(GICD_ISENABLER + reg * 4, 1 << bit);
    }

    fn disable(&mut self, irq_id: u32) {
        let reg = (irq_id / 32) as usize;
        let bit = irq_id % 32;
        self.gicd_write(GICD_ICENABLER + reg * 4, 1 << bit);
    }

    fn acknowledge(&mut self) -> u32 {
        self.gicc_read(GICC_IAR)
    }

    fn end_of_interrupt(&mut self, irq_id: u32) {
        self.gicc_write(GICC_EOIR, irq_id);
    }

    fn set_priority(&mut self, irq_id: u32, priority: u8) {
        let reg_offset = (irq_id / 4) as usize;
        let byte_offset = (irq_id % 4) as usize;
        let mut val = self.gicd_read(GICD_IPRIORITYR + reg_offset * 4);
        val &= !(0xFF << (byte_offset * 8));
        val |= (priority as u32) << (byte_offset * 8);
        self.gicd_write(GICD_IPRIORITYR + reg_offset * 4, val);
    }
}

pub fn init(gicd_base: usize, gicc_base: usize) {
    let mut gic = Gic400::new(gicd_base, gicc_base);
    gic.init();
    // SAFETY: Called once during single-core init before interrupts are enabled.
    unsafe { *GIC.0.get() = Some(gic) }
}

/// Initialize this core's GIC CPU interface. Called by secondary cores
/// after the distributor has been initialized by core 0.
pub fn init_cpu_interface() {
    // SAFETY: GIC struct is initialized; GICC registers are per-core banked.
    unsafe {
        if let Some(gic) = (*GIC.0.get()).as_mut() {
            gic.gicc_write(GICC_PMR, 0xFF);
            gic.gicc_write(GICC_CTLR, 1);
        }
    }
}

pub fn enable(irq_id: u32) {
    // SAFETY: GIC is initialized; single-core or spinlock-protected.
    unsafe {
        if let Some(gic) = (*GIC.0.get()).as_mut() {
            gic.enable(irq_id);
        }
    }
}

pub fn set_priority(irq_id: u32, priority: u8) {
    // SAFETY: Same as enable.
    unsafe {
        if let Some(gic) = (*GIC.0.get()).as_mut() {
            gic.set_priority(irq_id, priority);
        }
    }
}

/// Acknowledge an interrupt. Returns the full IAR value (INTID in bits [9:0],
/// source CPU in bits [12:10] for SGIs). Pass the full value to end_of_interrupt.
pub fn acknowledge() -> u32 {
    // SAFETY: GIC is initialized; called from IRQ context.
    unsafe {
        (*GIC.0.get())
            .as_mut()
            .map(|g| g.acknowledge())
            .unwrap_or(1023)
    }
}

pub fn end_of_interrupt(iar_value: u32) {
    // SAFETY: Same as acknowledge.
    unsafe {
        if let Some(gic) = (*GIC.0.get()).as_mut() {
            gic.end_of_interrupt(iar_value);
        }
    }
}

/// Send a Software Generated Interrupt (SGI) to a specific core.
pub fn send_sgi(target_core: u8, sgi_id: u8) {
    assert!(sgi_id < 16, "SGI ID must be 0-15");
    assert!((target_core as usize) < 4, "target core out of range");

    // GICD_SGIR format:
    //   [25:24] = Target list filter (0b00 = use target list in bits [23:16])
    //   [23:16] = CPU target list (bit per core)
    //   [3:0]   = SGI INTID
    let val = ((1u32 << target_core) << 16) | (sgi_id as u32);
    // SAFETY: GIC is initialized; GICD_SGIR is a write-only register.
    unsafe {
        if let Some(gic) = (*GIC.0.get()).as_mut() {
            gic.gicd_write(GICD_SGIR, val);
        }
    }
}
