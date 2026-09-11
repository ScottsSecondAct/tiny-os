//! Temperature Monitor — user-space application for tiny_os
//!
//! Runs at EL0. Reads SoC temperature via SYS_TEMPERATURE syscall every 5
//! seconds, tracks min/max/avg statistics, and prints periodic status updates
//! via SYS_WRITE. All hardware access is mediated through kernel syscalls.

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
fn sys_temperature() -> i32 { syscall(6, 0, 0) as i32 }

// ── String constants (must live in .user.text for EL0 access) ──────────────

#[link_section = ".user.text"]
static MSG_START: [u8; 29] = *b"[temp-monitor] started (EL0)\n";

#[link_section = ".user.text"]
static MSG_UNAVAIL: [u8; 34] = *b"[temp-monitor] sensor unavailable\n";

#[link_section = ".user.text"]
static LBL: [u8; 7] = *b"[temp] ";

#[link_section = ".user.text"]
static S_MIN: [u8; 7] = *b" | min ";

#[link_section = ".user.text"]
static S_MAX: [u8; 5] = *b" max ";

#[link_section = ".user.text"]
static S_AVG: [u8; 5] = *b" avg ";

#[link_section = ".user.text"]
static S_SEP: [u8; 3] = *b" | ";

#[link_section = ".user.text"]
static S_READINGS: [u8; 10] = *b" readings\n";

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

/// Write temperature in millidegrees as "25.0C".
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
pub fn temp_monitor_main(_arg: usize) -> ! {
    sys_write_raw(MSG_START.as_ptr(), 29);
    sys_delay(2000);

    let mut min: i32 = i32::MAX;
    let mut max: i32 = i32::MIN;
    let mut sum: i64 = 0;
    let mut count: u64 = 0;

    loop {
        let temp = sys_temperature();
        if temp >= 0 {
            count = count.wrapping_add(1);
            sum = sum.wrapping_add(temp as i64);
            if temp < min { min = temp; }
            if temp > max { max = temp; }
            let avg = if count > 0 { (sum / count as i64) as i32 } else { 0 };

            // "[temp] 25.0C | min 25.0C max 25.0C avg 25.0C | 42 readings\n"
            let mut buf: core::mem::MaybeUninit<[u8; 96]> = core::mem::MaybeUninit::uninit();
            let p = buf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;
            pos = wstatic(p, pos, LBL.as_ptr(), 7);
            pos = wtemp(p, pos, temp);
            pos = wstatic(p, pos, S_MIN.as_ptr(), 7);
            pos = wtemp(p, pos, min);
            pos = wstatic(p, pos, S_MAX.as_ptr(), 5);
            pos = wtemp(p, pos, max);
            pos = wstatic(p, pos, S_AVG.as_ptr(), 5);
            pos = wtemp(p, pos, avg);
            pos = wstatic(p, pos, S_SEP.as_ptr(), 3);
            pos = wu64(p, pos, count);
            pos = wstatic(p, pos, S_READINGS.as_ptr(), 10);
            sys_write_raw(p, pos);
        } else {
            sys_write_raw(MSG_UNAVAIL.as_ptr(), 34);
        }
        sys_delay(5000);
    }
}
