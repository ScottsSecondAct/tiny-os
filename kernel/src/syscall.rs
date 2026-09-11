use arch::aarch64::exceptions::TrapFrame;
use crate::{kprintln, sched};

const SYS_YIELD: u64 = 0;
const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_TASK_ID: u64 = 3;
const SYS_UPTIME: u64 = 4;
const SYS_EXIT: u64 = 5;

pub fn dispatch(tf: &mut TrapFrame) {
    let syscall_nr = tf.regs[8];
    let a0 = tf.regs[0];
    let a1 = tf.regs[1];

    let result: u64 = match syscall_nr {
        SYS_YIELD => {
            sched::task_yield();
            0
        }
        SYS_DELAY => {
            sched::delay(a0 as u32);
            0
        }
        SYS_WRITE => {
            let ptr = a0 as *const u8;
            let len = a1 as usize;
            if len <= 256 && !ptr.is_null() {
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    crate::print::_print_str(s);
                }
            }
            len as u64
        }
        SYS_TASK_ID => {
            sched::current_task_id() as u64
        }
        SYS_UPTIME => {
            arch::aarch64::exceptions::tick_count()
        }
        SYS_EXIT => {
            let id = sched::current_task_id();
            kprintln!("[syscall] task {} called exit", id);
            sched::task_terminate(id);
            0
        }
        _ => {
            kprintln!("[syscall] unknown syscall {}", syscall_nr);
            u64::MAX
        }
    };

    tf.regs[0] = result;
}
