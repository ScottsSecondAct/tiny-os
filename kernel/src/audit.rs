use crate::kprintln;
use crate::spinlock::SpinLock;
use arch::aarch64::{exceptions, smp};

const MAX_ENTRIES: usize = 64;
const ENTRY_SIZE: usize = 80;

#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum AuditEvent {
    Boot = 0,
    Shutdown = 1,
    AuthOk = 2,
    AuthFail = 3,
    FirewallDrop = 4,
    CapabilityDenied = 5,
    IntegrityOk = 6,
    IntegrityFail = 7,
    TaskCreated = 8,
    TaskTerminated = 9,
    RateLimited = 10,
}

impl AuditEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            AuditEvent::Boot => "BOOT",
            AuditEvent::Shutdown => "SHUTDOWN",
            AuditEvent::AuthOk => "AUTH_OK",
            AuditEvent::AuthFail => "AUTH_FAIL",
            AuditEvent::FirewallDrop => "FW_DROP",
            AuditEvent::CapabilityDenied => "CAP_DENY",
            AuditEvent::IntegrityOk => "INTEG_OK",
            AuditEvent::IntegrityFail => "INTEG_FAIL",
            AuditEvent::TaskCreated => "TASK_NEW",
            AuditEvent::TaskTerminated => "TASK_TERM",
            AuditEvent::RateLimited => "RATE_LIM",
        }
    }
}

#[derive(Clone, Copy)]
pub struct AuditEntry {
    pub tick: u64,
    pub core_id: u8,
    pub task_id: u8,
    pub event: AuditEvent,
    pub detail: [u8; 40],
    pub detail_len: u8,
    pub valid: bool,
}

impl AuditEntry {
    const fn empty() -> Self {
        Self {
            tick: 0,
            core_id: 0,
            task_id: 0,
            event: AuditEvent::Boot,
            detail: [0; 40],
            detail_len: 0,
            valid: false,
        }
    }
}

static LOCK: SpinLock = SpinLock::new();
static mut ENTRIES: [AuditEntry; MAX_ENTRIES] = [AuditEntry::empty(); MAX_ENTRIES];
static mut HEAD: usize = 0;
static mut COUNT: usize = 0;

pub fn log(event: AuditEvent, detail: &str) {
    let tick = exceptions::tick_count();
    let core_id = smp::core_id() as u8;
    let task_id = crate::sched::current_task_id();

    let mut entry = AuditEntry::empty();
    entry.tick = tick;
    entry.core_id = core_id;
    entry.task_id = task_id;
    entry.event = event;
    entry.valid = true;

    let copy_len = detail.len().min(40);
    entry.detail[..copy_len].copy_from_slice(&detail.as_bytes()[..copy_len]);
    entry.detail_len = copy_len as u8;

    let saved = LOCK.lock();
    unsafe {
        ENTRIES[HEAD] = entry;
        HEAD = (HEAD + 1) % MAX_ENTRIES;
        if COUNT < MAX_ENTRIES {
            COUNT += 1;
        }
    }
    LOCK.unlock(saved);
}

pub fn dump(max: usize) {
    let saved = LOCK.lock();
    let count = unsafe { COUNT };
    let head = unsafe { HEAD };
    let n = count.min(max);

    if n == 0 {
        LOCK.unlock(saved);
        kprintln!("audit: no entries");
        return;
    }

    let start = if head >= n {
        head - n
    } else {
        MAX_ENTRIES - (n - head)
    };

    for i in 0..n {
        let idx = (start + i) % MAX_ENTRIES;
        let e = unsafe { &ENTRIES[idx] };
        if !e.valid {
            continue;
        }
        let detail = core::str::from_utf8(&e.detail[..e.detail_len as usize]).unwrap_or("?");
        kprintln!(
            "[{:>8}] core{} task{:>2} {:<10} {}",
            e.tick,
            e.core_id,
            e.task_id,
            e.event.as_str(),
            detail
        );
    }

    LOCK.unlock(saved);
}

pub fn persist_to_fs() {
    let saved = LOCK.lock();
    let count = unsafe { COUNT };
    let head = unsafe { HEAD };
    LOCK.unlock(saved);

    if count == 0 {
        return;
    }

    let fd = match crate::fs::open("/audit.log", true) {
        Ok(f) => f,
        Err(_) => match crate::fs::create("/audit.log") {
            Ok(f) => f,
            Err(_) => return,
        },
    };

    let start = if head >= count {
        head - count
    } else {
        MAX_ENTRIES - (count - head)
    };

    for i in 0..count {
        let saved = LOCK.lock();
        let idx = (start + i) % MAX_ENTRIES;
        let e = unsafe { ENTRIES[idx] };
        LOCK.unlock(saved);
        if !e.valid {
            continue;
        }
        let detail = core::str::from_utf8(&e.detail[..e.detail_len as usize]).unwrap_or("?");
        let mut line = [0u8; ENTRY_SIZE];
        let len = fmt_entry(
            &mut line,
            e.tick,
            e.core_id,
            e.task_id,
            e.event.as_str(),
            detail,
        );
        let _ = crate::fs::write(fd, &line[..len]);
    }
    let _ = crate::fs::close(fd);
}

fn fmt_entry(
    buf: &mut [u8; ENTRY_SIZE],
    tick: u64,
    core: u8,
    task: u8,
    event: &str,
    detail: &str,
) -> usize {
    let mut pos = 0;
    pos += write_u64(&mut buf[pos..], tick);
    buf[pos] = b' ';
    pos += 1;
    buf[pos] = b'c';
    pos += 1;
    buf[pos] = b'0' + core;
    pos += 1;
    buf[pos] = b' ';
    pos += 1;
    buf[pos] = b't';
    pos += 1;
    pos += write_u8_dec(&mut buf[pos..], task);
    buf[pos] = b' ';
    pos += 1;
    let ev_bytes = event.as_bytes();
    let ev_len = ev_bytes.len().min(buf.len() - pos - detail.len() - 2);
    buf[pos..pos + ev_len].copy_from_slice(&ev_bytes[..ev_len]);
    pos += ev_len;
    buf[pos] = b' ';
    pos += 1;
    let det_bytes = detail.as_bytes();
    let det_len = det_bytes.len().min(buf.len() - pos - 1);
    buf[pos..pos + det_len].copy_from_slice(&det_bytes[..det_len]);
    pos += det_len;
    buf[pos] = b'\n';
    pos += 1;
    pos
}

fn write_u64(buf: &mut [u8], val: u64) -> usize {
    if val == 0 {
        buf[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 20];
    let mut n = val;
    let mut i = 0;
    while n > 0 {
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

fn write_u8_dec(buf: &mut [u8], val: u8) -> usize {
    write_u64(buf, val as u64)
}
