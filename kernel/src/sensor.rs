use crate::{kprintln, klog_info, klog_warn, fs, sched};
use arch::aarch64::{exceptions, mailbox};

const READING_INTERVAL_MS: u32 = 5000;
const LOG_INTERVAL_READINGS: usize = 12; // write to file every ~60s
const HISTORY_SIZE: usize = 60; // 5 minutes of history at 5s intervals
const ALERT_THRESHOLD_MC: i32 = 80_000; // 80°C

struct Reading {
    temp_mc: i32,
    timestamp_ms: u64,
}

static mut HISTORY: [Reading; HISTORY_SIZE] = {
    const EMPTY: Reading = Reading { temp_mc: 0, timestamp_ms: 0 };
    [EMPTY; HISTORY_SIZE]
};
static mut HISTORY_COUNT: usize = 0;
static mut HISTORY_HEAD: usize = 0;
static mut CURRENT_TEMP: i32 = 0;
static mut MIN_TEMP: i32 = i32::MAX;
static mut MAX_TEMP: i32 = i32::MIN;
static mut TOTAL_READINGS: u64 = 0;
static mut SUM_TEMP: i64 = 0;

pub fn sensor_task(_arg: usize) -> ! {
    kprintln!("[sensor] task started");
    sched::delay(2000);

    let mut readings_since_log: usize = 0;

    loop {
        match mailbox::get_temperature() {
            Some(temp_mc) => {
                let ts = exceptions::tick_count();
                record(temp_mc, ts);

                klog_info!("sensor", "{}.{}C",
                    temp_mc / 1000, ((temp_mc % 1000).abs()) / 100);

                if temp_mc >= ALERT_THRESHOLD_MC {
                    klog_warn!("sensor", "high temp {}.{}C >= {}C",
                        temp_mc / 1000, ((temp_mc % 1000).abs()) / 100,
                        ALERT_THRESHOLD_MC / 1000);
                }

                readings_since_log += 1;
                if readings_since_log >= LOG_INTERVAL_READINGS {
                    log_to_file();
                    readings_since_log = 0;
                }
            }
            None => {
                klog_info!("sensor", "temperature read unavailable");
            }
        }

        sched::delay(READING_INTERVAL_MS);
    }
}

fn record(temp_mc: i32, timestamp_ms: u64) {
    // SAFETY: Single writer (sensor_task).
    unsafe {
        HISTORY[HISTORY_HEAD] = Reading { temp_mc, timestamp_ms };
        HISTORY_HEAD = (HISTORY_HEAD + 1) % HISTORY_SIZE;
        if HISTORY_COUNT < HISTORY_SIZE {
            HISTORY_COUNT += 1;
        }
        CURRENT_TEMP = temp_mc;
        TOTAL_READINGS += 1;
        SUM_TEMP += temp_mc as i64;
        if temp_mc < MIN_TEMP { MIN_TEMP = temp_mc; }
        if temp_mc > MAX_TEMP { MAX_TEMP = temp_mc; }
    }
}

pub fn current() -> Option<i32> {
    unsafe {
        if TOTAL_READINGS == 0 { None } else { Some(CURRENT_TEMP) }
    }
}

/// Returns (min_mc, max_mc, avg_mc, total_readings).
pub fn stats() -> (i32, i32, i32, u64) {
    unsafe {
        if TOTAL_READINGS == 0 {
            return (0, 0, 0, 0);
        }
        let avg = (SUM_TEMP / TOTAL_READINGS as i64) as i32;
        (MIN_TEMP, MAX_TEMP, avg, TOTAL_READINGS)
    }
}

/// Print recent temperature history to the shell.
pub fn print_history(count: usize) {
    unsafe {
        if HISTORY_COUNT == 0 {
            kprintln!("  no readings yet");
            return;
        }
        let n = count.min(HISTORY_COUNT);
        let start = if HISTORY_HEAD >= n {
            HISTORY_HEAD - n
        } else {
            HISTORY_SIZE - (n - HISTORY_HEAD)
        };
        for i in 0..n {
            let idx = (start + i) % HISTORY_SIZE;
            let r = &HISTORY[idx];
            let secs = r.timestamp_ms / 1000;
            let mins = secs / 60;
            let s = secs % 60;
            kprintln!("  {:3}:{:02}  {}.{}C",
                mins, s, r.temp_mc / 1000, ((r.temp_mc % 1000).abs()) / 100);
        }
    }
}

fn log_to_file() {
    if !fs::is_mounted() {
        return;
    }
    let (min, max, avg, count) = stats();
    let uptime_s = exceptions::tick_count() / 1000;
    let cur = unsafe { CURRENT_TEMP };

    let mut buf = [0u8; 256];
    let mut pos = 0;
    pos = fmt_str(&mut buf, pos, "tiny_os sensor log\n");
    pos = fmt_str(&mut buf, pos, "current: ");
    pos = fmt_temp(&mut buf, pos, cur);
    pos = fmt_str(&mut buf, pos, "\nmin:     ");
    pos = fmt_temp(&mut buf, pos, min);
    pos = fmt_str(&mut buf, pos, "\nmax:     ");
    pos = fmt_temp(&mut buf, pos, max);
    pos = fmt_str(&mut buf, pos, "\navg:     ");
    pos = fmt_temp(&mut buf, pos, avg);
    pos = fmt_str(&mut buf, pos, "\nreadings: ");
    pos = fmt_u64(&mut buf, pos, count);
    pos = fmt_str(&mut buf, pos, "\nuptime:  ");
    pos = fmt_u64(&mut buf, pos, uptime_s);
    pos = fmt_str(&mut buf, pos, "s\n");

    match fs::create("/TEMP.LOG") {
        Ok(fd) => {
            let _ = fs::write(fd, &buf[..pos]);
            let _ = fs::close(fd);
        }
        Err(_) => {}
    }
}

fn fmt_str(buf: &mut [u8], pos: usize, s: &str) -> usize {
    let bytes = s.as_bytes();
    let n = bytes.len().min(buf.len() - pos);
    buf[pos..pos + n].copy_from_slice(&bytes[..n]);
    pos + n
}

fn fmt_temp(buf: &mut [u8], pos: usize, mc: i32) -> usize {
    let whole = mc / 1000;
    let frac = ((mc % 1000).abs()) / 100;
    let mut p = fmt_i32(buf, pos, whole);
    p = fmt_str(buf, p, ".");
    p = fmt_u64(buf, p, frac as u64);
    fmt_str(buf, p, "C")
}

fn fmt_i32(buf: &mut [u8], pos: usize, val: i32) -> usize {
    if val < 0 {
        let p = fmt_str(buf, pos, "-");
        fmt_u64(buf, p, (-(val as i64)) as u64)
    } else {
        fmt_u64(buf, pos, val as u64)
    }
}

fn fmt_u64(buf: &mut [u8], pos: usize, val: u64) -> usize {
    if val == 0 {
        return fmt_str(buf, pos, "0");
    }
    let mut digits = [0u8; 20];
    let mut n = 0usize;
    let mut v = val;
    while v > 0 {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    let space = buf.len() - pos;
    let count = n.min(space);
    for i in 0..count {
        buf[pos + i] = digits[n - 1 - i];
    }
    pos + count
}
