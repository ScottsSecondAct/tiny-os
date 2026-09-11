use crate::kprintln;
use crate::sched;
use arch::aarch64::exceptions::TrapFrame;
use arch::aarch64::{exceptions, gic, timer};

#[no_mangle]
extern "C" fn handle_irq(_tf: &mut TrapFrame) {
    let irq_id = gic::acknowledge();

    if irq_id < 1020 {
        if !exceptions::dispatch_irq(irq_id) {
            kprintln!("warning: unhandled IRQ {}", irq_id);
        }
        gic::end_of_interrupt(irq_id);

        // After handling a timer tick, let the scheduler check for preemption.
        if irq_id == timer::TIMER_IRQ_ID {
            sched::tick();
        }
    }
}

#[no_mangle]
extern "C" fn handle_sync(tf: &mut TrapFrame, from_el0: u64) {
    let esr = exceptions::read_esr();
    let ec = (esr >> 26) & 0x3F;
    let iss = esr & 0x1FF_FFFF;

    // SVC from AArch64 (EC=0x15): advance past the instruction and return.
    if ec == 0x15 {
        kprintln!("SVC #{} caught (EL{})", iss & 0xFFFF, if from_el0 == 0 { 1 } else { 0 });
        tf.elr_el1 += 4;
        return;
    }

    kprintln!(
        "\n*** SYNC EXCEPTION (EL{}) ***",
        if from_el0 == 0 { 1 } else { 0 }
    );
    kprintln!("  ESR_EL1:  {:#018x}  (EC={:#04x} ISS={:#09x})", esr, ec, iss);
    kprintln!("  ELR_EL1:  {:#018x}", tf.elr_el1);
    kprintln!("  SPSR_EL1: {:#018x}", tf.spsr_el1);

    if from_el0 == 0 {
        panic!("unhandled synchronous exception in EL1");
    }
}

#[no_mangle]
extern "C" fn handle_unhandled(tf: &mut TrapFrame) {
    let esr = exceptions::read_esr();
    kprintln!("\n*** UNHANDLED EXCEPTION ***");
    kprintln!("  ESR_EL1:  {:#018x}", esr);
    kprintln!("  ELR_EL1:  {:#018x}", tf.elr_el1);
    panic!("unhandled exception vector taken");
}
