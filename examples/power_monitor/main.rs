//! Power Monitor — user-space application for tiny_os
//!
//! Runs at EL0. Reads CPU frequency, voltage, and temperature via SYS_POWER
//! and SYS_TEMPERATURE syscalls to display a power management dashboard.
//! Gracefully degrades to temperature-only mode when SYS_POWER capability
//! is denied (CAP_POWER is not in CAP_USER_DEFAULT).
//!
//! All hardware access is mediated through kernel syscalls — no direct MMIO.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_TEMPERATURE: u64 = 6;
const SYS_POWER: u64 = 21;

// POWER operation codes
const POWER_GET_FREQ: u64 = 0;
const POWER_GET_MAX_FREQ: u64 = 2;
const POWER_GET_MIN_FREQ: u64 = 3;
const POWER_GET_VOLTAGE: u64 = 4;

// Error codes
const E_PERM: u64 = u64::MAX - 8;

// Thermal warning threshold: 70C in millidegrees
const THERMAL_WARN_MC: i32 = 70_000;

// Sampling interval in milliseconds
const SAMPLE_INTERVAL_MS: u32 = 8000;

// Summary interval in number of readings
const SUMMARY_INTERVAL: u64 = 10;

// ── Syscall interface ────────────────────────────────────────────────────────

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
fn syscall(nr: u64, a0: u64, a1: u64) -> u64 {
    syscall4(nr, a0, a1, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_delay(ms: u32) { syscall(SYS_DELAY, ms as u64, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall(SYS_WRITE, ptr as u64, len as u64); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_temperature() -> i32 { syscall(SYS_TEMPERATURE, 0, 0) as i32 }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_power(op: u64) -> u64 { syscall4(SYS_POWER, op, 0, 0, 0) }

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_START: [u8; 30] = *b"[power-monitor] started (EL0)\n";

#[link_section = ".user.text"]
static MSG_PROBE: [u8; 33] = *b"[power-monitor] probing power.. \n";

#[link_section = ".user.text"]
static MSG_POWER_OK: [u8; 38] = *b"[power-monitor] power syscalls: OK   \n";

#[link_section = ".user.text"]
static MSG_POWER_DENIED: [u8; 48] = *b"[power-monitor] power denied, temp-only mode   \n";

#[link_section = ".user.text"]
static MSG_THERMAL_WARN: [u8; 40] = *b"[power-monitor] THERMAL WARNING: >70C! \n";

#[link_section = ".user.text"]
static LBL_POWER: [u8; 8] = *b"[power] ";

#[link_section = ".user.text"]
static S_FREQ: [u8; 5] = *b"freq=";

#[link_section = ".user.text"]
static S_MHZ: [u8; 3] = *b"MHz";

#[link_section = ".user.text"]
static S_MIN: [u8; 5] = *b" min=";

#[link_section = ".user.text"]
static S_MAX: [u8; 5] = *b" max=";

#[link_section = ".user.text"]
static S_VOLT: [u8; 6] = *b" volt=";

#[link_section = ".user.text"]
static S_MV: [u8; 2] = *b"mV";

#[link_section = ".user.text"]
static S_TEMP: [u8; 6] = *b" temp=";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static S_FREQ_NA: [u8; 24] = *b"freq=N/A (no capability)";

#[link_section = ".user.text"]
static S_SUMMARY_HDR: [u8; 20] = *b"[power-monitor] === ";

#[link_section = ".user.text"]
static S_SUMMARY_READINGS: [u8; 19] = *b"-reading summary ==";

#[link_section = ".user.text"]
static S_SUMMARY_LBL: [u8; 18] = *b"[power-monitor]   ";

#[link_section = ".user.text"]
static S_TEMP_MIN: [u8; 9] = *b"temp min=";

#[link_section = ".user.text"]
static S_TEMP_MAX: [u8; 5] = *b" max=";

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

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn power_monitor_main(_arg: usize) -> ! {
    sys_write_raw(MSG_START.as_ptr(), MSG_START.len());
    sys_delay(1000);

    // Probe power capability
    sys_write_raw(MSG_PROBE.as_ptr(), MSG_PROBE.len());
    let probe = sys_power(POWER_GET_FREQ);
    let power_available = probe != E_PERM;

    if power_available {
        sys_write_raw(MSG_POWER_OK.as_ptr(), MSG_POWER_OK.len());
    } else {
        sys_write_raw(MSG_POWER_DENIED.as_ptr(), MSG_POWER_DENIED.len());
    }

    sys_delay(500);

    let mut min_temp: i32 = i32::MAX;
    let mut max_temp: i32 = i32::MIN;
    let mut count: u64 = 0;

    loop {
        let temp = sys_temperature();

        // Update min/max temperature tracking
        if temp >= 0 {
            if temp < min_temp { min_temp = temp; }
            if temp > max_temp { max_temp = temp; }
        }

        count = count.wrapping_add(1);

        // Build status line into stack buffer
        // Worst case: "[power] freq=2400MHz min=600MHz max=2400MHz volt=880mV temp=45.2C\n"
        // or:         "[power] freq=N/A (no capability) temp=45.2C\n"
        let mut buf: core::mem::MaybeUninit<[u8; 128]> = core::mem::MaybeUninit::uninit();
        let p = buf.as_mut_ptr() as *mut u8;
        let mut pos: usize = 0;

        pos = wstatic(p, pos, LBL_POWER.as_ptr(), LBL_POWER.len());

        if power_available {
            // Get current frequency (Hz -> MHz)
            let freq_hz = sys_power(POWER_GET_FREQ);
            let freq_mhz = freq_hz / 1_000_000;

            // Get min frequency
            let min_hz = sys_power(POWER_GET_MIN_FREQ);
            let min_mhz = min_hz / 1_000_000;

            // Get max frequency
            let max_hz = sys_power(POWER_GET_MAX_FREQ);
            let max_mhz = max_hz / 1_000_000;

            // Get voltage (microvolts -> millivolts)
            let voltage_uv = sys_power(POWER_GET_VOLTAGE);
            let voltage_mv = voltage_uv / 1000;

            // freq=2400MHz
            pos = wstatic(p, pos, S_FREQ.as_ptr(), S_FREQ.len());
            pos = wu64(p, pos, freq_mhz);
            pos = wstatic(p, pos, S_MHZ.as_ptr(), S_MHZ.len());

            // min=600MHz
            pos = wstatic(p, pos, S_MIN.as_ptr(), S_MIN.len());
            pos = wu64(p, pos, min_mhz);
            pos = wstatic(p, pos, S_MHZ.as_ptr(), S_MHZ.len());

            // max=2400MHz
            pos = wstatic(p, pos, S_MAX.as_ptr(), S_MAX.len());
            pos = wu64(p, pos, max_mhz);
            pos = wstatic(p, pos, S_MHZ.as_ptr(), S_MHZ.len());

            // volt=880mV
            pos = wstatic(p, pos, S_VOLT.as_ptr(), S_VOLT.len());
            pos = wu64(p, pos, voltage_mv);
            pos = wstatic(p, pos, S_MV.as_ptr(), S_MV.len());
        } else {
            // freq=N/A (no capability)
            pos = wstatic(p, pos, S_FREQ_NA.as_ptr(), S_FREQ_NA.len());
        }

        // temp=45.2C
        pos = wstatic(p, pos, S_TEMP.as_ptr(), S_TEMP.len());
        if temp >= 0 {
            pos = wtemp(p, pos, temp);
        } else {
            // Temperature unavailable (QEMU)
            unsafe {
                core::ptr::write_volatile(p.add(pos), b'N');
                core::ptr::write_volatile(p.add(pos + 1), b'/');
                core::ptr::write_volatile(p.add(pos + 2), b'A');
            }
            pos += 3;
        }

        pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());
        sys_write_raw(p, pos);

        // Thermal warning if temperature exceeds 70C
        if temp >= THERMAL_WARN_MC {
            sys_write_raw(MSG_THERMAL_WARN.as_ptr(), MSG_THERMAL_WARN.len());
        }

        // Print summary every SUMMARY_INTERVAL readings
        if count % SUMMARY_INTERVAL == 0 && min_temp != i32::MAX {
            // "[power-monitor] === 10-reading summary ==="
            let mut sbuf: core::mem::MaybeUninit<[u8; 96]> = core::mem::MaybeUninit::uninit();
            let sp = sbuf.as_mut_ptr() as *mut u8;
            let mut spos: usize = 0;

            spos = wstatic(sp, spos, S_SUMMARY_HDR.as_ptr(), S_SUMMARY_HDR.len());
            spos = wu64(sp, spos, count);
            spos = wstatic(sp, spos, S_SUMMARY_READINGS.as_ptr(), S_SUMMARY_READINGS.len());
            spos = wstatic(sp, spos, S_NL.as_ptr(), S_NL.len());
            sys_write_raw(sp, spos);

            // "[power-monitor]   temp min=25.0C max=46.1C"
            let mut tbuf: core::mem::MaybeUninit<[u8; 96]> = core::mem::MaybeUninit::uninit();
            let tp = tbuf.as_mut_ptr() as *mut u8;
            let mut tpos: usize = 0;

            tpos = wstatic(tp, tpos, S_SUMMARY_LBL.as_ptr(), S_SUMMARY_LBL.len());
            tpos = wstatic(tp, tpos, S_TEMP_MIN.as_ptr(), S_TEMP_MIN.len());
            tpos = wtemp(tp, tpos, min_temp);
            tpos = wstatic(tp, tpos, S_TEMP_MAX.as_ptr(), S_TEMP_MAX.len());
            tpos = wtemp(tp, tpos, max_temp);
            tpos = wstatic(tp, tpos, S_NL.as_ptr(), S_NL.len());
            sys_write_raw(tp, tpos);
        }

        sys_delay(SAMPLE_INTERVAL_MS);
    }
}
