use arch::rtc::{DateTime, RtcError};
use core::sync::atomic::{AtomicU64, AtomicBool, Ordering};

static EPOCH_SECS: AtomicU64 = AtomicU64::new(0);
static EPOCH_TICK: AtomicU64 = AtomicU64::new(0);
static ALARM_SECS: AtomicU64 = AtomicU64::new(0);
static ALARM_TRIGGERED: AtomicBool = AtomicBool::new(false);

pub fn get_time() -> Result<DateTime, RtcError> {
    let epoch = EPOCH_SECS.load(Ordering::Relaxed);
    let base_tick = EPOCH_TICK.load(Ordering::Relaxed);
    let now_tick = arch::aarch64::exceptions::tick_count();
    let elapsed_secs = (now_tick.saturating_sub(base_tick)) / 1000;
    Ok(unix_to_datetime(epoch + elapsed_secs))
}

pub fn set_time(dt: &DateTime) -> Result<(), RtcError> {
    if !valid_datetime(dt) {
        return Err(RtcError::InvalidTime);
    }
    let ts = datetime_to_unix(dt);
    let now_tick = arch::aarch64::exceptions::tick_count();
    EPOCH_SECS.store(ts, Ordering::Relaxed);
    EPOCH_TICK.store(now_tick, Ordering::Relaxed);
    Ok(())
}

pub fn set_alarm(dt: &DateTime) -> Result<(), RtcError> {
    if !valid_datetime(dt) {
        return Err(RtcError::InvalidTime);
    }
    let ts = datetime_to_unix(dt);
    ALARM_SECS.store(ts, Ordering::Relaxed);
    ALARM_TRIGGERED.store(false, Ordering::Relaxed);
    Ok(())
}

pub fn clear_alarm() -> Result<(), RtcError> {
    ALARM_SECS.store(0, Ordering::Relaxed);
    ALARM_TRIGGERED.store(false, Ordering::Relaxed);
    Ok(())
}

pub fn check_alarm() -> bool {
    let alarm = ALARM_SECS.load(Ordering::Relaxed);
    if alarm == 0 {
        return false;
    }
    if ALARM_TRIGGERED.load(Ordering::Relaxed) {
        return true;
    }
    if let Ok(now) = get_time() {
        let now_ts = datetime_to_unix(&now);
        if now_ts >= alarm {
            ALARM_TRIGGERED.store(true, Ordering::Relaxed);
            return true;
        }
    }
    false
}

fn valid_datetime(dt: &DateTime) -> bool {
    dt.month >= 1 && dt.month <= 12
        && dt.day >= 1 && dt.day <= days_in_month(dt.month, is_leap_year(dt.year))
        && dt.hour < 24
        && dt.minute < 60
        && dt.second < 60
        && dt.year >= 2000
}

fn is_leap_year(y: u16) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(m: u8, leap: bool) -> u8 {
    match m {
        1 => 31, 2 => if leap { 29 } else { 28 }, 3 => 31, 4 => 30,
        5 => 31, 6 => 30, 7 => 31, 8 => 31,
        9 => 30, 10 => 31, 11 => 30, 12 => 31,
        _ => 0,
    }
}

fn datetime_to_unix(dt: &DateTime) -> u64 {
    let mut days: u64 = 0;
    for y in 1970..dt.year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    for m in 1..dt.month {
        days += days_in_month(m, is_leap_year(dt.year)) as u64;
    }
    days += (dt.day - 1) as u64;
    days * 86400 + dt.hour as u64 * 3600 + dt.minute as u64 * 60 + dt.second as u64
}

fn unix_to_datetime(mut ts: u64) -> DateTime {
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
