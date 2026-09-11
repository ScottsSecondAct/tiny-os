use core::cell::UnsafeCell;
use core::fmt::{self, Write};
use crate::sched::CriticalSection;
use arch::aarch64::exceptions;

const LOG_MSG_SIZE: usize = 80;
const LOG_MODULE_SIZE: usize = 8;
const LOG_BUFFER_SIZE: usize = 64;

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN ",
            LogLevel::Info => "INFO ",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "error" => Some(LogLevel::Error),
            "warn" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct LogEntry {
    timestamp: u64,
    level: LogLevel,
    module: [u8; LOG_MODULE_SIZE],
    module_len: u8,
    msg: [u8; LOG_MSG_SIZE],
    msg_len: u8,
    used: bool,
}

impl LogEntry {
    const fn empty() -> Self {
        Self {
            timestamp: 0,
            level: LogLevel::Info,
            module: [0; LOG_MODULE_SIZE],
            module_len: 0,
            msg: [0; LOG_MSG_SIZE],
            msg_len: 0,
            used: false,
        }
    }
}

struct LogBuffer {
    entries: [LogEntry; LOG_BUFFER_SIZE],
    write_idx: usize,
    count: usize,
    min_level: LogLevel,
}

impl LogBuffer {
    const fn new() -> Self {
        Self {
            entries: [LogEntry::empty(); LOG_BUFFER_SIZE],
            write_idx: 0,
            count: 0,
            min_level: LogLevel::Info,
        }
    }
}

struct LogCell(UnsafeCell<LogBuffer>);
// SAFETY: Access protected by CriticalSection (single-core, IRQ masking).
unsafe impl Sync for LogCell {}

static LOG: LogCell = LogCell(UnsafeCell::new(LogBuffer::new()));

fn log_buf() -> &'static mut LogBuffer {
    // SAFETY: Caller holds CriticalSection.
    unsafe { &mut *LOG.0.get() }
}

pub fn set_level(level: LogLevel) {
    let _cs = CriticalSection::enter();
    log_buf().min_level = level;
}

pub fn get_level() -> LogLevel {
    let _cs = CriticalSection::enter();
    log_buf().min_level
}

pub fn log(level: LogLevel, module: &str, args: fmt::Arguments) {
    let _cs = CriticalSection::enter();
    let buf = log_buf();

    if (level as u8) > (buf.min_level as u8) {
        return;
    }

    let entry = &mut buf.entries[buf.write_idx];
    entry.timestamp = exceptions::tick_count();
    entry.level = level;
    entry.used = true;

    let mod_bytes = module.as_bytes();
    let mod_len = mod_bytes.len().min(LOG_MODULE_SIZE);
    entry.module[..mod_len].copy_from_slice(&mod_bytes[..mod_len]);
    entry.module_len = mod_len as u8;

    let mut writer = BufWriter { buf: &mut entry.msg, pos: 0 };
    let _ = writer.write_fmt(args);
    entry.msg_len = writer.pos as u8;

    buf.write_idx = (buf.write_idx + 1) % LOG_BUFFER_SIZE;
    if buf.count < LOG_BUFFER_SIZE {
        buf.count += 1;
    }

    // Print errors and warnings to UART immediately.
    if level <= LogLevel::Warn {
        let ts = entry.timestamp;
        let secs = ts / 1000;
        let frac = ts % 1000;
        let mod_str = core::str::from_utf8(&entry.module[..entry.module_len as usize]).unwrap_or("?");
        let msg_str = core::str::from_utf8(&entry.msg[..entry.msg_len as usize]).unwrap_or("?");
        crate::kprintln!("[{}.{:03}] {} [{}] {}", secs, frac, level.as_str(), mod_str, msg_str);
    }
}

/// Print recent log entries to the console.
pub fn dump(count: usize) {
    let _cs = CriticalSection::enter();
    let buf = log_buf();

    let n = count.min(buf.count);
    if n == 0 {
        crate::kprintln!("(no log entries)");
        return;
    }

    let start = if buf.count >= LOG_BUFFER_SIZE {
        (buf.write_idx + LOG_BUFFER_SIZE - n) % LOG_BUFFER_SIZE
    } else {
        buf.count - n
    };

    for i in 0..n {
        let idx = (start + i) % LOG_BUFFER_SIZE;
        let e = &buf.entries[idx];
        if !e.used {
            continue;
        }
        let secs = e.timestamp / 1000;
        let frac = e.timestamp % 1000;
        let mod_str = core::str::from_utf8(&e.module[..e.module_len as usize]).unwrap_or("?");
        let msg_str = core::str::from_utf8(&e.msg[..e.msg_len as usize]).unwrap_or("?");
        crate::kprintln!("[{}.{:03}] {} [{}] {}", secs, frac, e.level.as_str(), mod_str, msg_str);
    }
}

pub fn entry_count() -> usize {
    let _cs = CriticalSection::enter();
    log_buf().count
}

struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> fmt::Write for BufWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_write = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        Ok(())
    }
}

#[macro_export]
macro_rules! klog {
    ($level:expr, $module:expr, $($arg:tt)*) => {
        $crate::klog::log($level, $module, core::format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! klog_error {
    ($module:expr, $($arg:tt)*) => {
        $crate::klog!($crate::klog::LogLevel::Error, $module, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_warn {
    ($module:expr, $($arg:tt)*) => {
        $crate::klog!($crate::klog::LogLevel::Warn, $module, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_info {
    ($module:expr, $($arg:tt)*) => {
        $crate::klog!($crate::klog::LogLevel::Info, $module, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_debug {
    ($module:expr, $($arg:tt)*) => {
        $crate::klog!($crate::klog::LogLevel::Debug, $module, $($arg)*)
    };
}

#[macro_export]
macro_rules! klog_trace {
    ($module:expr, $($arg:tt)*) => {
        $crate::klog!($crate::klog::LogLevel::Trace, $module, $($arg)*)
    };
}
