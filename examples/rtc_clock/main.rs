//! RTC Clock — user-space application for tiny_os
//!
//! Runs at EL0. Reads the real-time clock via SYS_RTC syscalls, sets periodic
//! alarms (every 60 seconds), and displays time in a days-since-epoch format.
//! If the RTC capability is denied (CAP_RTC is not in CAP_USER_DEFAULT), the
//! app gracefully falls back to uptime-only mode.
//!
//! All hardware access is mediated through kernel syscalls — no direct MMIO.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_RTC: u64 = 17;

// RTC operations (X0 = op for multiplexed SYS_RTC)
const RTC_GET_TIME: u64 = 0;
const RTC_SET_ALARM: u64 = 2;

// Error codes
const E_PERM: u64 = u64::MAX - 8;

// ── DateTime struct (must match kernel arch::rtc::DateTime layout) ───────────

#[derive(Clone, Copy)]
struct DateTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

// ── Syscall interface ────────────────────────────────────────────────────────

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
fn syscall4(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
        );
    }
    ret
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_delay(ms: u32) { syscall(SYS_DELAY, ms as u64, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall(SYS_WRITE, ptr as u64, len as u64); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 { syscall(SYS_UPTIME, 0, 0) }

/// RTC_GET_TIME: kernel writes DateTime struct to the provided pointer.
/// Returns 0 on success, E_PERM if CAP_RTC is not granted.
#[link_section = ".user.text"]
#[inline(always)]
fn sys_rtc_get_time(dt: *mut DateTime) -> u64 {
    syscall4(SYS_RTC, RTC_GET_TIME, dt as u64, 0, 0)
}

/// RTC_SET_ALARM: kernel reads DateTime struct from the provided pointer.
/// Returns 0 on success.
#[link_section = ".user.text"]
#[inline(always)]
fn sys_rtc_set_alarm(dt: *const DateTime) -> u64 {
    syscall4(SYS_RTC, RTC_SET_ALARM, dt as u64, 0, 0)
}

// ── Calendar helpers (epoch <-> DateTime conversion) ─────────────────────────

#[link_section = ".user.text"]
#[inline(always)]
fn is_leap_year(y: u16) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn days_in_month(m: u8, leap: bool) -> u8 {
    match m {
        1 => 31, 2 => if leap { 29 } else { 28 }, 3 => 31, 4 => 30,
        5 => 31, 6 => 30, 7 => 31, 8 => 31,
        9 => 30, 10 => 31, 11 => 30, 12 => 31,
        _ => 0,
    }
}

/// Convert DateTime to Unix epoch seconds (matches kernel datetime_to_unix).
#[link_section = ".user.text"]
fn datetime_to_epoch(dt: &DateTime) -> u64 {
    let mut days: u64 = 0;
    let mut y: u16 = 1970;
    while y < dt.year {
        days += if is_leap_year(y) { 366 } else { 365 };
        y += 1;
    }
    let mut m: u8 = 1;
    while m < dt.month {
        days += days_in_month(m, is_leap_year(dt.year)) as u64;
        m += 1;
    }
    days += dt.day.wrapping_sub(1) as u64;
    days * 86400 + dt.hour as u64 * 3600 + dt.minute as u64 * 60 + dt.second as u64
}

/// Convert Unix epoch seconds to DateTime (matches kernel unix_to_datetime).
#[link_section = ".user.text"]
fn epoch_to_datetime(mut ts: u64) -> DateTime {
    let second = (ts % 60) as u8;
    ts /= 60;
    let minute = (ts % 60) as u8;
    ts /= 60;
    let hour = (ts % 24) as u8;
    let mut days = (ts / 24) as u32;

    let mut year: u16 = 1970;
    loop {
        let yd = if is_leap_year(year) { 366u32 } else { 365 };
        if days < yd {
            break;
        }
        days -= yd;
        year += 1;
    }

    let mut month: u8 = 1;
    loop {
        let md = days_in_month(month, is_leap_year(year)) as u32;
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    DateTime {
        year,
        month,
        day: days as u8 + 1,
        hour,
        minute,
        second,
    }
}

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_START: [u8; 26] = *b"[rtc-clock] started (EL0)\n";

#[link_section = ".user.text"]
static MSG_NO_RTC: [u8; 43] = *b"[rtc-clock] no CAP_RTC -- uptime-only mode\n";

#[link_section = ".user.text"]
static MSG_RTC_OK: [u8; 26] = *b"[rtc-clock] RTC available\n";

#[link_section = ".user.text"]
static MSG_ALARM_SET: [u8; 29] = *b"[rtc-clock] alarm set (+60s)\n";

#[link_section = ".user.text"]
static MSG_ALARM_FIRED: [u8; 21] = *b"[rtc] ALARM fired! (#";

#[link_section = ".user.text"]
static LBL: [u8; 6] = *b"[rtc] ";

#[link_section = ".user.text"]
static S_TPLUS: [u8; 2] = *b"T+";

#[link_section = ".user.text"]
static S_D_SPACE: [u8; 2] = *b"d ";

#[link_section = ".user.text"]
static S_COLON: [u8; 1] = *b":";

#[link_section = ".user.text"]
static S_EPOCH_OPEN: [u8; 8] = *b" (epoch ";

#[link_section = ".user.text"]
static S_CLOSE_UPTIME: [u8; 9] = *b") uptime=";

#[link_section = ".user.text"]
static S_S_NL: [u8; 2] = *b"s\n";

#[link_section = ".user.text"]
static S_UPTIME_EQ: [u8; 7] = *b"uptime=";

#[link_section = ".user.text"]
static S_NO_CAP_TAIL: [u8; 22] = *b"s (no RTC capability)\n";

#[link_section = ".user.text"]
static S_CLOSE_NL: [u8; 2] = *b")\n";

// ── Formatting helpers (volatile writes to avoid compiler memcpy) ────────────

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

/// Write a 2-digit zero-padded decimal value (00-99).
#[link_section = ".user.text"]
#[inline(always)]
fn wpad2(buf: *mut u8, pos: usize, val: u64) -> usize {
    let hi = b'0'.wrapping_add(((val / 10) % 10) as u8);
    let lo = b'0'.wrapping_add((val % 10) as u8);
    unsafe {
        core::ptr::write_volatile(buf.add(pos), hi);
        core::ptr::write_volatile(buf.add(pos.wrapping_add(1)), lo);
    }
    pos.wrapping_add(2)
}

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn rtc_clock_main(_arg: usize) -> ! {
    // 1. Print startup message
    sys_write_raw(MSG_START.as_ptr(), 26);
    sys_delay(1000);

    // 2. Probe RTC capability via RTC_GET_TIME
    let mut dt = DateTime {
        year: 0, month: 0, day: 0,
        hour: 0, minute: 0, second: 0,
    };
    let probe = sys_rtc_get_time(&mut dt);
    let rtc_ok = probe == 0;

    if probe == E_PERM {
        sys_write_raw(MSG_NO_RTC.as_ptr(), 43);
    } else if rtc_ok {
        sys_write_raw(MSG_RTC_OK.as_ptr(), 26);
    }

    // 3. If RTC available: read current time and set alarm for +60s
    let mut alarm_epoch: u64 = 0;
    let mut alarms_fired: u64 = 0;

    if rtc_ok {
        let epoch = datetime_to_epoch(&dt);
        alarm_epoch = epoch.wrapping_add(60);
        let alarm_dt = epoch_to_datetime(alarm_epoch);
        let ar = sys_rtc_set_alarm(&alarm_dt);
        if ar == 0 {
            sys_write_raw(MSG_ALARM_SET.as_ptr(), 29);
        }
    }

    // 4. Main loop — report every 5 seconds
    loop {
        let uptime_s = sys_uptime() / 1000;

        if rtc_ok {
            // Read current RTC time
            let mut now_dt = DateTime {
                year: 0, month: 0, day: 0,
                hour: 0, minute: 0, second: 0,
            };
            let r = sys_rtc_get_time(&mut now_dt);

            if r == 0 {
                let epoch = datetime_to_epoch(&now_dt);

                // Simple epoch-to-time decomposition (days/hours/minutes/seconds)
                let days = epoch / 86400;
                let hh = (epoch % 86400) / 3600;
                let mm = (epoch % 3600) / 60;
                let ss = epoch % 60;

                // Format: [rtc] T+DDDd HH:MM:SS (epoch NNNNNN) uptime=NNNNs
                let mut buf: core::mem::MaybeUninit<[u8; 128]> =
                    core::mem::MaybeUninit::uninit();
                let p = buf.as_mut_ptr() as *mut u8;
                let mut pos: usize = 0;

                pos = wstatic(p, pos, LBL.as_ptr(), 6);
                pos = wstatic(p, pos, S_TPLUS.as_ptr(), 2);
                pos = wu64(p, pos, days);
                pos = wstatic(p, pos, S_D_SPACE.as_ptr(), 2);
                pos = wpad2(p, pos, hh);
                pos = wstatic(p, pos, S_COLON.as_ptr(), 1);
                pos = wpad2(p, pos, mm);
                pos = wstatic(p, pos, S_COLON.as_ptr(), 1);
                pos = wpad2(p, pos, ss);
                pos = wstatic(p, pos, S_EPOCH_OPEN.as_ptr(), 8);
                pos = wu64(p, pos, epoch);
                pos = wstatic(p, pos, S_CLOSE_UPTIME.as_ptr(), 9);
                pos = wu64(p, pos, uptime_s);
                pos = wstatic(p, pos, S_S_NL.as_ptr(), 2);

                sys_write_raw(p, pos);

                // 5. Check if alarm has fired (current epoch >= alarm epoch)
                if alarm_epoch > 0 && epoch >= alarm_epoch {
                    alarms_fired = alarms_fired.wrapping_add(1);

                    // Print: [rtc] ALARM fired! (#N)
                    let mut abuf: core::mem::MaybeUninit<[u8; 48]> =
                        core::mem::MaybeUninit::uninit();
                    let ap = abuf.as_mut_ptr() as *mut u8;
                    let mut apos: usize = 0;
                    apos = wstatic(ap, apos, MSG_ALARM_FIRED.as_ptr(), 21);
                    apos = wu64(ap, apos, alarms_fired);
                    apos = wstatic(ap, apos, S_CLOSE_NL.as_ptr(), 2);
                    sys_write_raw(ap, apos);

                    // Set next alarm 60s from current time
                    alarm_epoch = epoch.wrapping_add(60);
                    let next_dt = epoch_to_datetime(alarm_epoch);
                    sys_rtc_set_alarm(&next_dt);
                }
            }
        } else {
            // Uptime-only mode (no RTC capability)
            // Format: [rtc] uptime=NNNNs (no RTC capability)
            let mut buf: core::mem::MaybeUninit<[u8; 64]> =
                core::mem::MaybeUninit::uninit();
            let p = buf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;

            pos = wstatic(p, pos, LBL.as_ptr(), 6);
            pos = wstatic(p, pos, S_UPTIME_EQ.as_ptr(), 7);
            pos = wu64(p, pos, uptime_s);
            pos = wstatic(p, pos, S_NO_CAP_TAIL.as_ptr(), 22);

            sys_write_raw(p, pos);
        }

        sys_delay(5000);
    }
}
