//! Machine Vision Inspector — user-space application for tiny_os
//!
//! Runs at EL0. Simulates an industrial machine vision quality inspection
//! pipeline: triggers frame capture via GPIO, processes a 32x32 grayscale
//! frame (threshold, blob detection, feature extraction), classifies pass/fail,
//! logs results to the FAT32 filesystem, and sends UDP alerts for rejects.
//!
//! All hardware access is mediated through kernel syscalls — no direct MMIO.
//!
//! Since no camera driver exists on bare metal, frames are generated
//! procedurally with deterministic defect patterns based on the inspection
//! counter. The SoC temperature (via SYS_TEMPERATURE) seeds slight frame
//! variation between runs.

use core::arch::asm;

// ── Syscall numbers ─────────────────────────────────────────────────────────

const SYS_YIELD: u64 = 0;
const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_TEMPERATURE: u64 = 6;
const SYS_FS: u64 = 10;
const SYS_NET: u64 = 11;
const SYS_GPIO: u64 = 14;

// FS operations
const FS_WRITE_OP: u64 = 2;
const FS_CLOSE: u64 = 3;
const FS_CREATE: u64 = 5;

// NET operations
const NET_SOCKET: u64 = 0;
const NET_SEND: u64 = 3;

// GPIO operations
const GPIO_SET_MODE: u64 = 0;
const GPIO_WRITE_OP: u64 = 2;

// Error codes
const E_PERM: u64 = u64::MAX - 8;
const E_NOSYS: u64 = u64::MAX;

// ── Vision configuration ────────────────────────────────────────────────────

const FRAME_W: usize = 32;
const FRAME_H: usize = 32;
const FRAME_SIZE: usize = FRAME_W * FRAME_H;
const THRESHOLD: u8 = 128;
const CYCLE_MS: u32 = 500;

const AREA_MIN: u32 = 200;
const AREA_MAX: u32 = 800;
const AREA_IDEAL: u32 = 500;
const CENTER_X: u32 = 16;
const CENTER_Y: u32 = 16;
const CENTER_MAX_DIST: u32 = 4;

const PIN_TRIGGER: u8 = 5;
const PIN_STROBE: u8 = 6;
const GPIO_MODE_OUTPUT: u8 = 1;

const STATS_INTERVAL: u64 = 10;

// Classification verdicts
const REASON_PASS: u8 = 0;
const REASON_NO_OBJECT: u8 = 1;
const REASON_MULTIPLE: u8 = 2;
const REASON_UNDERSIZED: u8 = 3;
const REASON_OVERSIZED: u8 = 4;
const REASON_OFF_CENTER: u8 = 5;

// ── Syscall interface ───────────────────────────────────────────────────────

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
fn syscall2(nr: u64, a0: u64, a1: u64) -> u64 {
    syscall4(nr, a0, a1, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_yield() { syscall2(SYS_YIELD, 0, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_delay(ms: u32) { syscall2(SYS_DELAY, ms as u64, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall2(SYS_WRITE, ptr as u64, len as u64); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 { syscall2(SYS_UPTIME, 0, 0) }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_temperature() -> i32 { syscall2(SYS_TEMPERATURE, 0, 0) as i32 }

// FS syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_create(path: *const u8, path_len: usize) -> u64 {
    syscall4(SYS_FS, FS_CREATE, path as u64, path_len as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_write(fd: u64, data: *const u8, len: usize) -> u64 {
    syscall4(SYS_FS, FS_WRITE_OP, fd, data as u64, len as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_close(fd: u64) -> u64 {
    syscall4(SYS_FS, FS_CLOSE, fd, 0, 0)
}

// NET syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_socket(sock_type: u64) -> u64 {
    syscall4(SYS_NET, NET_SOCKET, sock_type, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_send(fd: u64, data: *const u8, len: usize) -> u64 {
    syscall4(SYS_NET, NET_SEND, fd, data as u64, len as u64)
}

// GPIO syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_set_mode(pin: u8, mode: u8) -> u64 {
    syscall4(SYS_GPIO, GPIO_SET_MODE, pin as u64, mode as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_write(pin: u8, value: u8) -> u64 {
    syscall4(SYS_GPIO, GPIO_WRITE_OP, pin as u64, value as u64, 0)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

#[link_section = ".user.text"]
#[inline(always)]
fn is_error(val: u64) -> bool {
    val >= E_NOSYS.wrapping_sub(10)
}

// ── String constants (must live in .user.text for EL0 access) ───────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 40] = *b"[vision] machine vision inspector (EL0)\n";

#[link_section = ".user.text"]
static MSG_SIM_MODE: [u8; 43] = *b"[vision] GPIO unavailable, simulation mode\n";

#[link_section = ".user.text"]
static MSG_INIT_GPIO: [u8; 31] = *b"[vision] init trigger/strobe.. ";

#[link_section = ".user.text"]
static MSG_INIT_NET: [u8; 29] = *b"[vision] init alert socket.. ";

#[link_section = ".user.text"]
static MSG_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_SIMULATED: [u8; 10] = *b"simulated\n";

#[link_section = ".user.text"]
static MSG_FAIL: [u8; 5] = *b"fail\n";

#[link_section = ".user.text"]
static MSG_RUNNING: [u8; 26] = *b"[vision] pipeline running\n";

#[link_section = ".user.text"]
static S_LABEL: [u8; 9] = *b"[vision] ";

#[link_section = ".user.text"]
static S_BRACKET_OPEN: [u8; 1] = *b"[";

#[link_section = ".user.text"]
static S_BRACKET_CLOSE_SP: [u8; 2] = *b"] ";

#[link_section = ".user.text"]
static S_PASS_SCORE: [u8; 11] = *b"PASS score=";

#[link_section = ".user.text"]
static S_FAIL_COLON: [u8; 5] = *b"FAIL:";

#[link_section = ".user.text"]
static S_NO_OBJECT: [u8; 9] = *b"no_object";

#[link_section = ".user.text"]
static S_MULTIPLE: [u8; 8] = *b"multiple";

#[link_section = ".user.text"]
static S_UNDERSIZED: [u8; 10] = *b"undersized";

#[link_section = ".user.text"]
static S_OVERSIZED: [u8; 9] = *b"oversized";

#[link_section = ".user.text"]
static S_OFF_CENTER: [u8; 10] = *b"off_center";

#[link_section = ".user.text"]
static S_AREA: [u8; 6] = *b" area=";

#[link_section = ".user.text"]
static S_BLOBS: [u8; 7] = *b" blobs=";

#[link_section = ".user.text"]
static S_CX: [u8; 4] = *b" cx=";

#[link_section = ".user.text"]
static S_CY: [u8; 4] = *b" cy=";

#[link_section = ".user.text"]
static S_TEMP_EQ: [u8; 3] = *b" t=";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static S_STAT_INSP: [u8; 19] = *b"[vision] inspected=";

#[link_section = ".user.text"]
static S_STAT_PASS: [u8; 6] = *b" pass=";

#[link_section = ".user.text"]
static S_STAT_FAIL: [u8; 6] = *b" fail=";

#[link_section = ".user.text"]
static S_STAT_YIELD: [u8; 7] = *b" yield=";

#[link_section = ".user.text"]
static S_STAT_PCT_AVG: [u8; 11] = *b"% avg_time=";

#[link_section = ".user.text"]
static S_STAT_MS_NL: [u8; 3] = *b"ms\n";

#[link_section = ".user.text"]
static LOG_PATH: [u8; 10] = *b"VISION.TXT";

#[link_section = ".user.text"]
static S_ALERT_INSP: [u8; 11] = *b"ALERT insp=";

#[link_section = ".user.text"]
static S_ALERT_REASON: [u8; 8] = *b" reason=";

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

/// Write a 5-digit zero-padded number (e.g., 42 -> "00042").
#[link_section = ".user.text"]
#[inline(always)]
fn wpad5(buf: *mut u8, pos: usize, val: u64) -> usize {
    let d4 = ((val / 10000) % 10) as u8;
    let d3 = ((val / 1000) % 10) as u8;
    let d2 = ((val / 100) % 10) as u8;
    let d1 = ((val / 10) % 10) as u8;
    let d0 = (val % 10) as u8;
    unsafe {
        core::ptr::write_volatile(buf.add(pos), b'0'.wrapping_add(d4));
        core::ptr::write_volatile(buf.add(pos + 1), b'0'.wrapping_add(d3));
        core::ptr::write_volatile(buf.add(pos + 2), b'0'.wrapping_add(d2));
        core::ptr::write_volatile(buf.add(pos + 3), b'0'.wrapping_add(d1));
        core::ptr::write_volatile(buf.add(pos + 4), b'0'.wrapping_add(d0));
    }
    pos + 5
}

// ── Frame metrics ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct FrameMetrics {
    area: u32,
    blob_count: u32,
    cx: u32,
    cy: u32,
}

// ── Vision pipeline ─────────────────────────────────────────────────────────

/// Draw a filled rectangle of bright pixels into the frame buffer.
#[link_section = ".user.text"]
#[inline(always)]
fn draw_rect(frame: *mut u8, x0: usize, y0: usize, x1: usize, y1: usize) {
    let mut y = y0;
    while y <= y1 && y < FRAME_H {
        let mut x = x0;
        while x <= x1 && x < FRAME_W {
            unsafe { core::ptr::write_volatile(frame.add(y * FRAME_W + x), 200); }
            x += 1;
        }
        y += 1;
    }
}

/// Generate a synthetic 32x32 grayscale frame with deterministic defect
/// patterns based on the inspection counter.
///
/// - Normal: single bright ~20x20 rectangle near center (area ~400)
/// - Every 7th: two separate blobs (FAIL: multiple objects)
/// - Every 13th: oversized blob ~30x30 (FAIL: area > 800)
/// - Every 19th: undersized blob ~10x10 (FAIL: area < 200)
/// - Every 23rd: off-center blob (FAIL: centroid far from center)
///
/// The `seed` parameter (from SYS_TEMPERATURE) adds slight variation to
/// normal frames across different boot sessions.
#[link_section = ".user.text"]
fn generate_frame(frame: *mut u8, count: u64, seed: u64) {
    // Clear frame to black (background).
    let mut i: usize = 0;
    while i < FRAME_SIZE {
        unsafe { core::ptr::write_volatile(frame.add(i), 0u8); }
        i += 1;
    }

    let c = count as u32;

    if c % 7 == 0 {
        // ── Defect: two blobs ──
        // First blob: (3,3)-(12,14) = 10x12 = 120 pixels
        draw_rect(frame, 3, 3, 12, 14);
        // Second blob: (20,18)-(29,29) = 10x12 = 120 pixels
        draw_rect(frame, 20, 18, 29, 29);
    } else if c % 13 == 0 {
        // ── Defect: oversized blob ──
        // (1,1)-(30,30) = 30x30 = 900 pixels (> 800)
        draw_rect(frame, 1, 1, 30, 30);
    } else if c % 19 == 0 {
        // ── Defect: undersized blob ──
        // (12,12)-(19,19) = 8x8 = 64 pixels (< 200)
        draw_rect(frame, 12, 12, 19, 19);
    } else if c % 23 == 0 {
        // ── Defect: off-center blob ──
        // (0,0)-(19,19) = 20x20 = 400 pixels, centroid at ~(9,9)
        draw_rect(frame, 0, 0, 19, 19);
    } else {
        // ── Normal: single centered blob ──
        // Slight position variation based on counter bits and temperature seed.
        let cs = count.wrapping_add(seed) as u32;
        let ox = ((cs & 3) as i32) - 1;        // -1, 0, 1, or 2
        let oy = (((cs >> 2) & 3) as i32) - 1; // -1, 0, 1, or 2
        let x0 = (6 + ox) as usize;    // 5..8
        let y0 = (6 + oy) as usize;    // 5..8
        let x1 = (25 + ox) as usize;   // 24..27
        let y1 = (25 + oy) as usize;   // 24..27
        // 20x20 = 400 pixels, centroid near (15,15)
        draw_rect(frame, x0, y0, x1, y1);
    }
}

/// Threshold the frame in-place: pixel > THRESHOLD becomes 1, else 0.
#[link_section = ".user.text"]
fn threshold_inplace(frame: *mut u8) {
    let mut i: usize = 0;
    while i < FRAME_SIZE {
        let val = unsafe { core::ptr::read_volatile(frame.add(i)) };
        let bin = if val > THRESHOLD { 1u8 } else { 0u8 };
        unsafe { core::ptr::write_volatile(frame.add(i), bin); }
        i += 1;
    }
}

/// Analyze a binary (0/1) frame: count area, blobs, and compute centroid.
///
/// Blob detection uses 4-connectivity: a foreground pixel with neither its
/// left neighbor nor above neighbor being foreground starts a new blob.
/// This is exact for rectangular blobs that do not touch diagonally.
#[link_section = ".user.text"]
fn analyze_frame(frame: *const u8) -> FrameMetrics {
    let mut area: u32 = 0;
    let mut sum_x: u32 = 0;
    let mut sum_y: u32 = 0;
    let mut blob_count: u32 = 0;

    let mut y: u32 = 0;
    while y < FRAME_H as u32 {
        let mut x: u32 = 0;
        while x < FRAME_W as u32 {
            let idx = (y * FRAME_W as u32 + x) as usize;
            let val = unsafe { core::ptr::read_volatile(frame.add(idx)) };
            if val != 0 {
                area += 1;
                sum_x += x;
                sum_y += y;

                // 4-connectivity blob counting: new blob if neither
                // left nor above neighbor is foreground.
                let left_fg = if x > 0 {
                    let li = (y * FRAME_W as u32 + x - 1) as usize;
                    let lv = unsafe { core::ptr::read_volatile(frame.add(li)) };
                    lv != 0
                } else {
                    false
                };
                let above_fg = if y > 0 {
                    let ai = ((y - 1) * FRAME_W as u32 + x) as usize;
                    let av = unsafe { core::ptr::read_volatile(frame.add(ai)) };
                    av != 0
                } else {
                    false
                };
                if !left_fg && !above_fg {
                    blob_count += 1;
                }
            }
            x += 1;
        }
        y += 1;
    }

    let cx = if area > 0 { sum_x / area } else { CENTER_X };
    let cy = if area > 0 { sum_y / area } else { CENTER_Y };

    FrameMetrics { area, blob_count, cx, cy }
}

/// Classify an inspection result as PASS or FAIL with a confidence score.
///
/// PASS criteria: exactly 1 blob, area in [200, 800], centroid within
/// 4 pixels of frame center (16, 16).
///
/// Confidence (0-100): starts at 100, penalized by area deviation from
/// ideal (500) and centroid distance from center.
#[link_section = ".user.text"]
fn classify(m: &FrameMetrics) -> (u8, u32) {
    if m.area == 0 {
        return (REASON_NO_OBJECT, 0);
    }
    if m.blob_count > 1 {
        return (REASON_MULTIPLE, 0);
    }
    if m.area < AREA_MIN {
        return (REASON_UNDERSIZED, 0);
    }
    if m.area > AREA_MAX {
        return (REASON_OVERSIZED, 0);
    }

    let dx = if m.cx > CENTER_X { m.cx - CENTER_X } else { CENTER_X - m.cx };
    let dy = if m.cy > CENTER_Y { m.cy - CENTER_Y } else { CENTER_Y - m.cy };
    if dx > CENTER_MAX_DIST || dy > CENTER_MAX_DIST {
        return (REASON_OFF_CENTER, 0);
    }

    // PASS: compute confidence score.
    let mut conf: u32 = 100;
    let area_dev = if m.area > AREA_IDEAL { m.area - AREA_IDEAL } else { AREA_IDEAL - m.area };
    let area_pen = area_dev / 10;
    conf = if area_pen < conf { conf - area_pen } else { 0 };
    let center_pen = (dx + dy) * 3;
    conf = if center_pen < conf { conf - center_pen } else { 0 };

    (REASON_PASS, conf)
}

/// Return the string pointer and length for a fail reason code.
#[link_section = ".user.text"]
#[inline(always)]
fn reason_str(reason: u8) -> (*const u8, usize) {
    match reason {
        REASON_NO_OBJECT => (S_NO_OBJECT.as_ptr(), 9),
        REASON_MULTIPLE  => (S_MULTIPLE.as_ptr(), 8),
        REASON_UNDERSIZED => (S_UNDERSIZED.as_ptr(), 10),
        REASON_OVERSIZED => (S_OVERSIZED.as_ptr(), 9),
        REASON_OFF_CENTER => (S_OFF_CENTER.as_ptr(), 10),
        _ => (S_NO_OBJECT.as_ptr(), 9),
    }
}

// ── Output formatting ───────────────────────────────────────────────────────

/// Format an inspection result line into the buffer.
/// PASS: "[NNNNN] PASS score=87 area=400 blobs=1 cx=15 cy=14 t=45\n"
/// FAIL: "[NNNNN] FAIL:multiple area=240 blobs=2 cx=16 cy=16 t=45\n"
/// Returns the number of bytes written.
#[link_section = ".user.text"]
fn format_result(buf: *mut u8, count: u64, reason: u8, confidence: u32,
                 m: &FrameMetrics, temp_c: u32) -> usize {
    let mut pos: usize = 0;

    // "[NNNNN] "
    pos = wstatic(buf, pos, S_BRACKET_OPEN.as_ptr(), 1);
    pos = wpad5(buf, pos, count);
    pos = wstatic(buf, pos, S_BRACKET_CLOSE_SP.as_ptr(), 2);

    if reason == REASON_PASS {
        // "PASS score=NN"
        pos = wstatic(buf, pos, S_PASS_SCORE.as_ptr(), 11);
        pos = wu64(buf, pos, confidence as u64);
    } else {
        // "FAIL:reason"
        pos = wstatic(buf, pos, S_FAIL_COLON.as_ptr(), 5);
        let (rptr, rlen) = reason_str(reason);
        pos = wstatic(buf, pos, rptr, rlen);
    }

    // " area=N blobs=N cx=N cy=N t=N\n"
    pos = wstatic(buf, pos, S_AREA.as_ptr(), 6);
    pos = wu64(buf, pos, m.area as u64);
    pos = wstatic(buf, pos, S_BLOBS.as_ptr(), 7);
    pos = wu64(buf, pos, m.blob_count as u64);
    pos = wstatic(buf, pos, S_CX.as_ptr(), 4);
    pos = wu64(buf, pos, m.cx as u64);
    pos = wstatic(buf, pos, S_CY.as_ptr(), 4);
    pos = wu64(buf, pos, m.cy as u64);
    pos = wstatic(buf, pos, S_TEMP_EQ.as_ptr(), 3);
    pos = wu64(buf, pos, temp_c as u64);
    pos = wstatic(buf, pos, S_NL.as_ptr(), 1);

    pos
}

/// Format a UDP alert packet for a failed inspection.
/// "ALERT insp=NNNNN reason=off_center area=400 blobs=1"
#[link_section = ".user.text"]
fn format_alert(buf: *mut u8, count: u64, reason: u8, m: &FrameMetrics) -> usize {
    let mut pos: usize = 0;

    pos = wstatic(buf, pos, S_ALERT_INSP.as_ptr(), 11);
    pos = wu64(buf, pos, count);
    pos = wstatic(buf, pos, S_ALERT_REASON.as_ptr(), 8);
    let (rptr, rlen) = reason_str(reason);
    pos = wstatic(buf, pos, rptr, rlen);
    pos = wstatic(buf, pos, S_AREA.as_ptr(), 6);
    pos = wu64(buf, pos, m.area as u64);
    pos = wstatic(buf, pos, S_BLOBS.as_ptr(), 7);
    pos = wu64(buf, pos, m.blob_count as u64);

    pos
}

/// Format the statistics dashboard line.
/// "[vision] inspected=100 pass=94 fail=6 yield=94% avg_time=5ms\n"
#[link_section = ".user.text"]
fn format_stats(buf: *mut u8, count: u64, pass: u64, fail: u64,
                yield_pct: u64, avg_ms: u64) -> usize {
    let mut pos: usize = 0;

    pos = wstatic(buf, pos, S_STAT_INSP.as_ptr(), 19);
    pos = wu64(buf, pos, count);
    pos = wstatic(buf, pos, S_STAT_PASS.as_ptr(), 6);
    pos = wu64(buf, pos, pass);
    pos = wstatic(buf, pos, S_STAT_FAIL.as_ptr(), 6);
    pos = wu64(buf, pos, fail);
    pos = wstatic(buf, pos, S_STAT_YIELD.as_ptr(), 7);
    pos = wu64(buf, pos, yield_pct);
    pos = wstatic(buf, pos, S_STAT_PCT_AVG.as_ptr(), 11);
    pos = wu64(buf, pos, avg_ms);
    pos = wstatic(buf, pos, S_STAT_MS_NL.as_ptr(), 3);

    pos
}

// ── Application entry point ─────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn machine_vision_main(_arg: usize) -> ! {
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // ── Initialize GPIO for trigger (pin 5) and strobe (pin 6) ──
    sys_write_raw(MSG_INIT_GPIO.as_ptr(), MSG_INIT_GPIO.len());
    let trig_ret = sys_gpio_set_mode(PIN_TRIGGER, GPIO_MODE_OUTPUT);
    let gpio_ok = !is_error(trig_ret);
    if gpio_ok {
        sys_gpio_set_mode(PIN_STROBE, GPIO_MODE_OUTPUT);
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_SIMULATED.as_ptr(), MSG_SIMULATED.len());
        sys_write_raw(MSG_SIM_MODE.as_ptr(), MSG_SIM_MODE.len());
    }

    // ── Initialize UDP socket for reject alerts (port 6000) ──
    sys_write_raw(MSG_INIT_NET.as_ptr(), MSG_INIT_NET.len());
    let sock_fd = sys_net_socket(0); // UDP
    let net_ok = !is_error(sock_fd);
    if net_ok {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_FAIL.as_ptr(), MSG_FAIL.len());
    }

    // ── Read SoC temperature for frame variation seed ──
    let temp_raw = sys_temperature();
    let temp_seed: u64 = if temp_raw >= 0 { temp_raw as u64 } else { 42 };

    sys_write_raw(MSG_RUNNING.as_ptr(), MSG_RUNNING.len());

    // ── Statistics tracking ──
    let mut count: u64 = 0;
    let mut pass_count: u64 = 0;
    let mut fail_count: u64 = 0;
    let mut min_time: u64 = u64::MAX;
    let mut max_time: u64 = 0;
    let mut sum_time: u64 = 0;

    loop {
        count = count.wrapping_add(1);
        let t_start = sys_uptime();

        // ── Stage 1: Frame acquisition ──
        // Fire GPIO trigger/strobe sequence if hardware is available.
        if gpio_ok {
            sys_gpio_write(PIN_STROBE, 1);  // illumination on
            sys_gpio_write(PIN_TRIGGER, 1); // trigger capture
            sys_delay(10);                   // exposure time
            sys_gpio_write(PIN_TRIGGER, 0); // trigger off
            sys_gpio_write(PIN_STROBE, 0);  // illumination off
        }

        // Generate simulated frame (1024 bytes on stack).
        let mut frame: core::mem::MaybeUninit<[u8; FRAME_SIZE]> =
            core::mem::MaybeUninit::uninit();
        let fp = frame.as_mut_ptr() as *mut u8;
        generate_frame(fp, count, temp_seed);

        // ── Stage 2: Threshold (in-place, grayscale -> binary) ──
        threshold_inplace(fp);

        // ── Stage 3: Feature extraction & blob detection ──
        let metrics = analyze_frame(fp);

        // ── Stage 4: Classification ──
        let (reason, confidence) = classify(&metrics);
        let passed = reason == REASON_PASS;

        // Read SoC temperature for result log (in whole degrees C).
        let temp_now = sys_temperature();
        let temp_c: u32 = if temp_now > 0 { (temp_now as u32) / 1000 } else { 0 };

        // ── Update timing statistics ──
        let t_end = sys_uptime();
        let proc_time = t_end.wrapping_sub(t_start);

        if passed {
            pass_count = pass_count.wrapping_add(1);
        } else {
            fail_count = fail_count.wrapping_add(1);
        }
        if proc_time < min_time { min_time = proc_time; }
        if proc_time > max_time { max_time = proc_time; }
        sum_time = sum_time.wrapping_add(proc_time);

        // ── Stage 5: Format and output result ──
        let mut outbuf: core::mem::MaybeUninit<[u8; 128]> =
            core::mem::MaybeUninit::uninit();
        let bp = outbuf.as_mut_ptr() as *mut u8;

        let result_len = format_result(bp, count, reason, confidence, &metrics, temp_c);

        // Print to console with "[vision] " prefix.
        sys_write_raw(S_LABEL.as_ptr(), S_LABEL.len());
        sys_write_raw(bp, result_len);

        // Log to /VISION.TXT on the FAT32 filesystem.
        let fd = sys_fs_create(LOG_PATH.as_ptr(), LOG_PATH.len());
        if !is_error(fd) {
            sys_fs_write(fd, bp, result_len);
            sys_fs_close(fd);
        }

        // ── Stage 6: Network alert on FAIL ──
        if !passed && net_ok {
            let alert_len = format_alert(bp, count, reason, &metrics);
            sys_net_send(sock_fd, bp, alert_len);
        }

        // ── Stage 7: Statistics dashboard (every 10 inspections) ──
        if count % STATS_INTERVAL == 0 {
            let yield_pct = if count > 0 {
                pass_count * 100 / count
            } else {
                0
            };
            let avg_ms = if count > 0 {
                sum_time / count
            } else {
                0
            };
            let stats_len = format_stats(bp, count, pass_count, fail_count,
                                         yield_pct, avg_ms);
            sys_write_raw(bp, stats_len);
        }

        // ── Pace to target cycle time (500ms = 2 inspections/sec) ──
        let elapsed = sys_uptime().wrapping_sub(t_start);
        if elapsed < CYCLE_MS as u64 {
            sys_delay((CYCLE_MS as u64 - elapsed) as u32);
        } else {
            sys_yield();
        }
    }
}
