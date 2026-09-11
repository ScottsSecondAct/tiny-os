use core::arch::asm;

#[inline(always)]
fn syscall(nr: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inout("x0") a0 => ret,
            in("x1") a1,
            lateout("x2") _,
            lateout("x3") _,
        );
    }
    ret
}

#[allow(dead_code)]
#[inline(always)]
fn sys_yield() {
    syscall(0, 0, 0);
}

#[inline(always)]
fn sys_delay(ms: u32) {
    syscall(1, ms as u64, 0);
}

#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) {
    syscall(2, ptr as u64, len as u64);
}

#[inline(always)]
fn sys_task_id() -> u64 {
    syscall(3, 0, 0)
}

#[allow(dead_code)]
#[inline(always)]
fn sys_uptime() -> u64 {
    syscall(4, 0, 0)
}

#[allow(dead_code)]
#[inline(always)]
fn sys_exit() -> ! {
    syscall(5, 0, 0);
    loop { unsafe { asm!("wfe"); } }
}

// String data in .user.text (EL0-accessible), not .rodata (kernel-only).
#[link_section = ".user.text"]
static MSG_TASK: [u8; 12] = *b"[user] task ";
#[link_section = ".user.text"]
static MSG_EL0: [u8; 16] = *b" started at EL0\n";
#[link_section = ".user.text"]
static MSG_TICK: [u8; 12] = *b"[user] tick ";

/// Write u32 as decimal + newline via sys_write. Uses only stack + volatile ops
/// to prevent the compiler from emitting memcpy or overflow-panic calls.
#[link_section = ".user.text"]
#[inline(always)]
fn write_num_newline(mut n: u32) {
    let mut buf: [u8; 11] = [0; 11];
    let ptr = buf.as_mut_ptr();
    if n == 0 {
        unsafe {
            core::ptr::write_volatile(ptr, b'0');
            core::ptr::write_volatile(ptr.add(1), b'\n');
        }
        sys_write_raw(ptr, 2);
        return;
    }
    let mut tmp: [u8; 10] = [0; 10];
    let tp = tmp.as_mut_ptr();
    let mut dlen: usize = 0;
    while n > 0 {
        unsafe {
            core::ptr::write_volatile(
                tp.add(dlen),
                b'0'.wrapping_add((n % 10) as u8),
            );
        }
        n /= 10;
        dlen = dlen.wrapping_add(1);
    }
    let mut pos: usize = 0;
    let mut i = dlen;
    while i > 0 {
        i = i.wrapping_sub(1);
        unsafe {
            let c = core::ptr::read_volatile(tp.add(i));
            core::ptr::write_volatile(ptr.add(pos), c);
        }
        pos = pos.wrapping_add(1);
    }
    unsafe { core::ptr::write_volatile(ptr.add(pos), b'\n'); }
    pos = pos.wrapping_add(1);
    sys_write_raw(ptr, pos);
}

#[link_section = ".user.text"]
pub fn user_demo(_arg: usize) -> ! {
    let id = sys_task_id();

    // Write startup message as 3 separate writes (no local buffer copy needed).
    sys_write_raw(MSG_TASK.as_ptr(), 12);
    let d = [b'0'.wrapping_add(id as u8)];
    sys_write_raw(d.as_ptr(), 1);
    sys_write_raw(MSG_EL0.as_ptr(), 16);

    let mut counter: u32 = 0;
    loop {
        counter = counter.wrapping_add(1);
        sys_write_raw(MSG_TICK.as_ptr(), 12);
        write_num_newline(counter);
        sys_delay(2000);
    }
}
