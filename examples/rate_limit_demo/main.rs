//! Rate Limit Demo — user-space application for tiny_os
//!
//! Runs at EL0. Demonstrates Phase 14's per-task syscall rate limiting by
//! deliberately bursting syscalls to trigger the throttle, then recovering
//! after the sliding window resets. Educational example showing how the
//! kernel protects itself from runaway user-space syscall storms.
//!
//! The rate limiter allows 1000 syscalls per 1000 ms sliding window. Once
//! the limit is reached, subsequent syscalls return E_RATE_LIMIT until the
//! window slides past.

use core::arch::asm;

// -- Syscall numbers ----------------------------------------------------------

const SYS_YIELD: u64 = 0;
const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_TASK_ID: u64 = 3;
const SYS_UPTIME: u64 = 4;

// -- Error codes --------------------------------------------------------------

const E_RATE_LIMIT: u64 = u64::MAX - 9;

// -- Syscall interface --------------------------------------------------------

#[link_section = ".user.text"]
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

#[link_section = ".user.text"]
#[inline(always)]
fn sys_yield() -> u64 {
    syscall(SYS_YIELD, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_delay(ms: u32) {
    syscall(SYS_DELAY, ms as u64, 0);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) {
    syscall(SYS_WRITE, ptr as u64, len as u64);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_task_id() -> u64 {
    syscall(SYS_TASK_ID, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 {
    syscall(SYS_UPTIME, 0, 0)
}

// -- Rate limit check ---------------------------------------------------------

#[link_section = ".user.text"]
#[inline(always)]
fn is_rate_limited(ret: u64) -> bool {
    ret == E_RATE_LIMIT
}

// -- String constants (must live in .user.text for EL0 access) ----------------

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 51] = *b"[rate-limit] syscall rate limit demo started (EL0)\n";

#[link_section = ".user.text"]
static MSG_TASK_ID: [u8; 20] = *b"[rate-limit] task = ";

#[link_section = ".user.text"]
static MSG_LIMIT_INFO: [u8; 49] = *b"[rate-limit] Rate limit: 1000 syscalls/second   \n";

#[link_section = ".user.text"]
static MSG_PHASE1: [u8; 49] = *b"[rate-limit] --- Phase 1: normal operation ---  \n";

#[link_section = ".user.text"]
static MSG_NORMAL_PRE: [u8; 22] = *b"[rate-limit]   Normal:";

#[link_section = ".user.text"]
static MSG_NORMAL_MID: [u8; 4] = *b" of ";

#[link_section = ".user.text"]
static MSG_NORMAL_OK: [u8; 15] = *b" syscalls OK  \n";

#[link_section = ".user.text"]
static MSG_PHASE2: [u8; 50] = *b"[rate-limit] --- Phase 2: burst test ---         \n";

#[link_section = ".user.text"]
static MSG_BURST_PRE: [u8; 21] = *b"[rate-limit]   Burst:";

#[link_section = ".user.text"]
static MSG_BURST_SUC: [u8; 12] = *b" succeeded, ";

#[link_section = ".user.text"]
static MSG_BURST_RL: [u8; 14] = *b" rate-limited\n";

#[link_section = ".user.text"]
static MSG_PHASE3: [u8; 50] = *b"[rate-limit] --- Phase 3: recovery ---           \n";

#[link_section = ".user.text"]
static MSG_RECOVERY_WAIT: [u8; 40] = *b"[rate-limit]   waiting 2s for window...\n";

#[link_section = ".user.text"]
static MSG_RECOVERY_OK: [u8; 43] = *b"[rate-limit]   Recovery: syscalls working!\n";

#[link_section = ".user.text"]
static MSG_RECOVERY_FAIL: [u8; 49] = *b"[rate-limit]   Recovery: still rate-limited (!!)\n";

#[link_section = ".user.text"]
static MSG_CYCLE: [u8; 44] = *b"[rate-limit] cycle complete, waiting 30s...\n";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static S_SPACE: [u8; 1] = *b" ";

// -- Formatting helpers (volatile writes to avoid compiler memcpy) ------------

#[link_section = ".user.text"]
#[inline(always)]
fn wstatic(buf: *mut u8, pos: usize, src: *const u8, len: usize) -> usize {
    let mut i = 0;
    while i < len {
        unsafe {
            let b = core::ptr::read_volatile(src.add(i));
            core::ptr::write_volatile(buf.add(pos + i), b);
        }
        i += 1;
    }
    pos + len
}

#[link_section = ".user.text"]
#[inline(always)]
fn wu64(buf: *mut u8, pos: usize, val: u64) -> usize {
    if val == 0 {
        unsafe {
            core::ptr::write_volatile(buf.add(pos), b'0');
        }
        return pos + 1;
    }
    let mut tmp = [0u8; 20];
    let tp = tmp.as_mut_ptr();
    let mut n: usize = 0;
    let mut v = val;
    while v > 0 {
        unsafe {
            core::ptr::write_volatile(tp.add(n), b'0'.wrapping_add((v % 10) as u8));
        }
        v /= 10;
        n = n.wrapping_add(1);
    }
    let mut p = pos;
    let mut i = n;
    while i > 0 {
        i = i.wrapping_sub(1);
        unsafe {
            let c = core::ptr::read_volatile(tp.add(i));
            core::ptr::write_volatile(buf.add(p), c);
        }
        p = p.wrapping_add(1);
    }
    p
}

// -- Application entry point --------------------------------------------------

#[link_section = ".user.text"]
pub fn rate_limit_demo_main(_arg: usize) -> ! {
    // Print startup banner
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // Print task ID
    {
        let tid = sys_task_id();
        let mut buf: core::mem::MaybeUninit<[u8; 32]> = core::mem::MaybeUninit::uninit();
        let p = buf.as_mut_ptr() as *mut u8;
        let mut pos: usize = 0;
        pos = wstatic(p, pos, MSG_TASK_ID.as_ptr(), MSG_TASK_ID.len());
        pos = wu64(p, pos, tid);
        pos = wstatic(p, pos, S_NL.as_ptr(), 1);
        sys_write_raw(p, pos);
    }

    // Print rate limit configuration
    sys_write_raw(MSG_LIMIT_INFO.as_ptr(), MSG_LIMIT_INFO.len());
    sys_delay(1000);

    loop {
        // ---- Phase 1: Normal operation ----
        sys_write_raw(MSG_PHASE1.as_ptr(), MSG_PHASE1.len());

        let normal_total: u64 = 10;
        let mut normal_ok: u64 = 0;
        let mut i: u64 = 0;
        while i < normal_total {
            let ret = sys_yield();
            if !is_rate_limited(ret) {
                normal_ok = normal_ok.wrapping_add(1);
            }
            sys_delay(500); // spread across 5 seconds (10 x 500ms)
            i = i.wrapping_add(1);
        }

        // Print: "Normal: X of Y syscalls OK"
        {
            let mut buf: core::mem::MaybeUninit<[u8; 64]> = core::mem::MaybeUninit::uninit();
            let p = buf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;
            pos = wstatic(p, pos, MSG_NORMAL_PRE.as_ptr(), MSG_NORMAL_PRE.len());
            pos = wstatic(p, pos, S_SPACE.as_ptr(), 1);
            pos = wu64(p, pos, normal_ok);
            pos = wstatic(p, pos, MSG_NORMAL_MID.as_ptr(), MSG_NORMAL_MID.len());
            pos = wu64(p, pos, normal_total);
            pos = wstatic(p, pos, MSG_NORMAL_OK.as_ptr(), MSG_NORMAL_OK.len());
            sys_write_raw(p, pos);
        }

        sys_delay(1000);

        // ---- Phase 2: Burst test ----
        sys_write_raw(MSG_PHASE2.as_ptr(), MSG_PHASE2.len());

        let burst_total: u64 = 600;
        let mut burst_ok: u64 = 0;
        let mut burst_rl: u64 = 0;
        let mut j: u64 = 0;
        while j < burst_total {
            let ret = sys_yield();
            if is_rate_limited(ret) {
                burst_rl = burst_rl.wrapping_add(1);
            } else {
                burst_ok = burst_ok.wrapping_add(1);
            }
            j = j.wrapping_add(1);
        }

        // Print: "Burst: X succeeded, Y rate-limited"
        {
            let mut buf: core::mem::MaybeUninit<[u8; 64]> = core::mem::MaybeUninit::uninit();
            let p = buf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;
            pos = wstatic(p, pos, MSG_BURST_PRE.as_ptr(), MSG_BURST_PRE.len());
            pos = wstatic(p, pos, S_SPACE.as_ptr(), 1);
            pos = wu64(p, pos, burst_ok);
            pos = wstatic(p, pos, MSG_BURST_SUC.as_ptr(), MSG_BURST_SUC.len());
            pos = wu64(p, pos, burst_rl);
            pos = wstatic(p, pos, MSG_BURST_RL.as_ptr(), MSG_BURST_RL.len());
            sys_write_raw(p, pos);
        }

        // ---- Phase 3: Recovery ----
        sys_write_raw(MSG_PHASE3.as_ptr(), MSG_PHASE3.len());
        sys_write_raw(MSG_RECOVERY_WAIT.as_ptr(), MSG_RECOVERY_WAIT.len());

        // Wait for the sliding window to reset
        sys_delay(2000);

        // Verify syscalls work again
        let recovery_ret = sys_yield();
        if !is_rate_limited(recovery_ret) {
            sys_write_raw(MSG_RECOVERY_OK.as_ptr(), MSG_RECOVERY_OK.len());
        } else {
            sys_write_raw(MSG_RECOVERY_FAIL.as_ptr(), MSG_RECOVERY_FAIL.len());
        }

        // Cycle complete
        sys_write_raw(MSG_CYCLE.as_ptr(), MSG_CYCLE.len());
        sys_delay(30000);
    }
}
