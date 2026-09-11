/// Host tests for RTC datetime conversion functions.
///
/// These replicate the pure-logic datetime_to_unix / unix_to_datetime
/// functions from kernel::rtc so they can be tested on the host.

struct DateTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
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

#[test]
fn unix_epoch_zero() {
    let dt = unix_to_datetime(0);
    assert_eq!(dt.year, 1970);
    assert_eq!(dt.month, 1);
    assert_eq!(dt.day, 1);
    assert_eq!(dt.hour, 0);
    assert_eq!(dt.minute, 0);
    assert_eq!(dt.second, 0);
}

#[test]
fn unix_epoch_roundtrip() {
    let dt = DateTime { year: 2025, month: 6, day: 15, hour: 12, minute: 30, second: 45 };
    let ts = datetime_to_unix(&dt);
    let dt2 = unix_to_datetime(ts);
    assert_eq!(dt2.year, 2025);
    assert_eq!(dt2.month, 6);
    assert_eq!(dt2.day, 15);
    assert_eq!(dt2.hour, 12);
    assert_eq!(dt2.minute, 30);
    assert_eq!(dt2.second, 45);
}

#[test]
fn leap_year_feb29() {
    let dt = DateTime { year: 2024, month: 2, day: 29, hour: 0, minute: 0, second: 0 };
    let ts = datetime_to_unix(&dt);
    let dt2 = unix_to_datetime(ts);
    assert_eq!(dt2.year, 2024);
    assert_eq!(dt2.month, 2);
    assert_eq!(dt2.day, 29);
}

#[test]
fn non_leap_year() {
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2000));
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(2023));
    assert!(is_leap_year(2400));
}

#[test]
fn known_timestamp() {
    // 2000-01-01 00:00:00 = 946684800
    let dt = DateTime { year: 2000, month: 1, day: 1, hour: 0, minute: 0, second: 0 };
    assert_eq!(datetime_to_unix(&dt), 946684800);
}

#[test]
fn year_end_boundary() {
    let dt = DateTime { year: 2024, month: 12, day: 31, hour: 23, minute: 59, second: 59 };
    let ts = datetime_to_unix(&dt);
    let dt2 = unix_to_datetime(ts);
    assert_eq!(dt2.year, 2024);
    assert_eq!(dt2.month, 12);
    assert_eq!(dt2.day, 31);
    assert_eq!(dt2.hour, 23);
    assert_eq!(dt2.minute, 59);
    assert_eq!(dt2.second, 59);

    let next = unix_to_datetime(ts + 1);
    assert_eq!(next.year, 2025);
    assert_eq!(next.month, 1);
    assert_eq!(next.day, 1);
}

#[test]
fn days_in_month_all() {
    let expected = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (i, &e) in expected.iter().enumerate() {
        assert_eq!(days_in_month((i + 1) as u8, false), e);
    }
    assert_eq!(days_in_month(2, true), 29);
}
