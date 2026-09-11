//! System Dashboard — user-space application for tiny_os
//!
//! Runs at EL0. Periodically reads system status (uptime, temperature, task ID)
//! and prints a formatted dashboard report every 10 seconds via SYS_WRITE.
//! Tracks the number of dashboard updates printed. All hardware access is
//! mediated through kernel syscalls — no direct MMIO.

use core::arch::asm;

// ── Syscall interface ──────────────────────────────────────────────────────

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

#[inline(always)]
fn sys_delay(ms: u32) { syscall(1, ms as u64, 0); }

#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall(2, ptr as u64, len as u64); }

#[inline(always)]
fn sys_task_id() -> u64 { syscall(3, 0, 0) }

#[inline(always)]
fn sys_uptime() -> u64 { syscall(4, 0, 0) }

#[inline(always)]
fn sys_temperature() -> i32 { syscall(6, 0, 0) as i32 }

// ── String constants (must live in .user.text for EL0 access) ──────────────

#[link_section = ".user.text"]
static MSG_START: [u8; 26] = *b"[dashboard] started (EL0)\n";

#[link_section = ".user.text"]
static HDR: [u8; 26] = *b"--- tiny-os dashboard ---\n";

#[link_section = ".user.text"]
static FTR: [u8; 26] = *b"-------------------------\n";

#[link_section = ".user.text"]
static LBL_TASK: [u8; 8] = *b"  task: ";

#[link_section = ".user.text"]
static LBL_UPTIME: [u8; 10] = *b"  uptime: ";

#[link_section = ".user.text"]
static LBL_TEMP: [u8; 8] = *b"  temp: ";

#[link_section = ".user.text"]
static LBL_UPDATES: [u8; 11] = *b"  updates: ";

#[link_section = ".user.text"]
static LBL_STATUS: [u8; 10] = *b"  status: ";

#[link_section = ".user.text"]
static S_OK: [u8; 3] = *b"OK\n";

#[link_section = ".user.text"]
static S_NO_SENSOR: [u8; 10] = *b"no sensor\n";

#[link_section = ".user.text"]
static S_S_NL: [u8; 2] = *b"s\n";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

// ── Formatting helpers (volatile writes to avoid compiler memcpy) ──────────

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
        unsafe { core::ptr::write_volatile(buf.add(pos), b'0'); }
        return pos + 1;
    }
    let mut tmp = [0u8; 20];
    let tp = tmp.as_mut_ptr();
    let mut n: usize = 0;
    let mut v = val;
    while v > 0 {
        unsafe { core::ptr::write_volatile(tp.add(n), b'0'.wrapping_add((v % 10) as u8)); }
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

/// Write temperature in millidegrees as "45.2C".
#[link_section = ".user.text"]
#[inline(always)]
fn wtemp(buf: *mut u8, pos: usize, mc: i32) -> usize {
    let mut p = pos;
    let (whole, frac) = if mc < 0 {
        unsafe { core::ptr::write_volatile(buf.add(p), b'-'); }
        p = p.wrapping_add(1);
        let abs = (-(mc as i64)) as u64;
        (abs / 1000, ((abs % 1000) / 100) as u8)
    } else {
        ((mc as u64) / 1000, (((mc as u64) % 1000) / 100) as u8)
    };
    p = wu64(buf, p, whole);
    unsafe {
        core::ptr::write_volatile(buf.add(p), b'.');
        core::ptr::write_volatile(buf.add(p.wrapping_add(1)), b'0'.wrapping_add(frac));
        core::ptr::write_volatile(buf.add(p.wrapping_add(2)), b'C');
    }
    p.wrapping_add(3)
}

// ── Application entry point ────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn system_dashboard_main(_arg: usize) -> ! {
    sys_write_raw(MSG_START.as_ptr(), MSG_START.len());
    sys_delay(1000);

    let task_id = sys_task_id();
    let mut updates: u64 = 0;

    loop {
        updates = updates.wrapping_add(1);

        let uptime_ms = sys_uptime();
        let uptime_s = uptime_ms / 1000;
        let temp = sys_temperature();
        let temp_ok = temp >= 0;

        // Format dashboard into a single buffer and write once
        // Max line: header(25) + task(8+20+1) + uptime(10+20+2) + temp(8+12+1)
        //         + updates(11+20+1) + status(10+10+1) + footer(26) < 192
        let mut buf: core::mem::MaybeUninit<[u8; 192]> = core::mem::MaybeUninit::uninit();
        let p = buf.as_mut_ptr() as *mut u8;
        let mut pos: usize = 0;

        // --- tiny-os dashboard ---
        pos = wstatic(p, pos, HDR.as_ptr(), HDR.len());

        //   task: <id>
        pos = wstatic(p, pos, LBL_TASK.as_ptr(), LBL_TASK.len());
        pos = wu64(p, pos, task_id);
        pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());

        //   uptime: <seconds>s
        pos = wstatic(p, pos, LBL_UPTIME.as_ptr(), LBL_UPTIME.len());
        pos = wu64(p, pos, uptime_s);
        pos = wstatic(p, pos, S_S_NL.as_ptr(), S_S_NL.len());

        //   temp: 45.2C  (or "no sensor")
        pos = wstatic(p, pos, LBL_TEMP.as_ptr(), LBL_TEMP.len());
        if temp_ok {
            pos = wtemp(p, pos, temp);
            pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());
        } else {
            pos = wstatic(p, pos, S_NO_SENSOR.as_ptr(), S_NO_SENSOR.len());
        }

        //   updates: <count>
        pos = wstatic(p, pos, LBL_UPDATES.as_ptr(), LBL_UPDATES.len());
        pos = wu64(p, pos, updates);
        pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());

        //   status: OK
        pos = wstatic(p, pos, LBL_STATUS.as_ptr(), LBL_STATUS.len());
        pos = wstatic(p, pos, S_OK.as_ptr(), S_OK.len());

        // -------------------------
        pos = wstatic(p, pos, FTR.as_ptr(), FTR.len());

        sys_write_raw(p, pos);
        sys_delay(10000);
    }
}
