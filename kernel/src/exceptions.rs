use crate::kprintln;
use crate::sched;
use arch::aarch64::exceptions::TrapFrame;
use arch::aarch64::{exceptions, gic, timer};

#[no_mangle]
extern "C" fn handle_irq(_tf: &mut TrapFrame) {
    let iar = gic::acknowledge();
    let irq_id = iar & 0x3FF;

    if irq_id < 1020 {
        if irq_id == gic::SGI_RESCHEDULE {
            // IPI reschedule — handled after EOI below.
        } else if !exceptions::dispatch_irq(irq_id) {
            kprintln!("warning: unhandled IRQ {}", irq_id);
        }
        gic::end_of_interrupt(iar);

        if irq_id == timer::TIMER_IRQ_ID {
            sched::tick();
        } else if irq_id == gic::SGI_RESCHEDULE {
            sched::ipi_reschedule();
        }
    }
}

#[no_mangle]
extern "C" fn handle_sync(tf: &mut TrapFrame, from_el0: u64) {
    let esr = exceptions::read_esr();
    let ec = (esr >> 26) & 0x3F;
    let iss = esr & 0x1FF_FFFF;

    if ec == 0x15 {
        if from_el0 != 0 {
            crate::syscall::dispatch(tf);
            return;
        }
        kprintln!("SVC #{} from EL1 (ignored)", iss & 0xFFFF);
        return;
    }

    if from_el0 != 0 {
        let far: u64;
        unsafe { core::arch::asm!("mrs {}, far_el1", out(reg) far) };
        let id = sched::current_task_id();
        kprintln!(
            "\n*** EL0 FAULT: task {} ***\n  EC={:#04x} ISS={:#09x}\n  ELR={:#018x} FAR={:#018x}",
            id, ec, iss, tf.elr_el1, far
        );
        kprintln!("  LR={:#018x} SP_EL0={:#018x}", tf.regs[30], tf.sp_el0);
        kprintln!("  x0={:#018x} x8={:#018x} x9={:#018x} x10={:#018x}",
            tf.regs[0], tf.regs[8], tf.regs[9], tf.regs[10]);
        sched::task_terminate(id);
        return;
    }

    kprintln!(
        "\n*** SYNC EXCEPTION (EL1) ***"
    );
    kprintln!("  ESR_EL1:  {:#018x}  (EC={:#04x} ISS={:#09x})", esr, ec, iss);
    kprintln!("  ELR_EL1:  {:#018x}", tf.elr_el1);
    kprintln!("  SPSR_EL1: {:#018x}", tf.spsr_el1);

    panic!("unhandled synchronous exception in EL1");
}

#[no_mangle]
extern "C" fn handle_unhandled(tf: &mut TrapFrame) {
    let esr = exceptions::read_esr();
    kprintln!("\n*** UNHANDLED EXCEPTION ***");
    kprintln!("  ESR_EL1:  {:#018x}", esr);
    kprintln!("  ELR_EL1:  {:#018x}", tf.elr_el1);
    panic!("unhandled exception vector taken");
}
