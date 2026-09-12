//! Data Logger — user-space application for tiny_os
//!
//! Runs at EL0. Creates a log file on the FAT32 filesystem and writes
//! timestamped entries every 15 seconds, demonstrating filesystem syscall
//! operations from user space. Each entry records the uptime (ms) and SoC
//! temperature (millidegrees Celsius). Falls back to console-only logging
//! when filesystem access is denied or unavailable.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_TEMPERATURE: u64 = 6;
const SYS_FS: u64 = 10;

// FS operations
const FS_WRITE_OP: u64 = 2;
const FS_CLOSE: u64 = 3;
const FS_CREATE: u64 = 5;

// Error sentinels
const E_NOSYS: u64 = u64::MAX;
const E_PERM: u64 = u64::MAX - 8;

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
fn sys_delay(ms: u32) {
    syscall(SYS_DELAY, ms as u64, 0);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) {
    syscall(SYS_WRITE, ptr as u64, len as u64);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 {
    syscall(SYS_UPTIME, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_temperature() -> i32 {
    syscall(SYS_TEMPERATURE, 0, 0) as i32
}

// FS syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_create(path_ptr: *const u8, path_len: usize) -> u64 {
    syscall4(SYS_FS, FS_CREATE, path_ptr as u64, path_len as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_write(fd: u64, buf_ptr: *const u8, buf_len: usize) -> u64 {
    syscall4(SYS_FS, FS_WRITE_OP, fd, buf_ptr as u64, buf_len as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_close(fd: u64) -> u64 {
    syscall4(SYS_FS, FS_CLOSE, fd, 0, 0)
}

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 29] = *b"[data-logger] started at EL0\n";

#[link_section = ".user.text"]
static MSG_CREATING: [u8; 34] = *b"[data-logger] creating /LOG.TXT.. ";

#[link_section = ".user.text"]
static MSG_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_FAIL_PERM: [u8; 26] = *b"denied (no FS capability)\n";

#[link_section = ".user.text"]
static MSG_FAIL_OTHER: [u8; 7] = *b"failed\n";

#[link_section = ".user.text"]
static MSG_FALLBACK: [u8; 43] = *b"[data-logger] falling back to console only\n";

#[link_section = ".user.text"]
static MSG_LOGGING: [u8; 37] = *b"[data-logger] logging to /LOG.TXT ok\n";

#[link_section = ".user.text"]
static LOG_PATH: [u8; 7] = *b"LOG.TXT";

#[link_section = ".user.text"]
static LBL_ENTRY: [u8; 1] = *b"[";

#[link_section = ".user.text"]
static LBL_CLOSE: [u8; 2] = *b"] ";

#[link_section = ".user.text"]
static LBL_TEMP: [u8; 5] = *b"temp=";

#[link_section = ".user.text"]
static LBL_STATUS: [u8; 10] = *b" status=ok";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static LBL_CONSOLE: [u8; 14] = *b"[data-logger] ";

#[link_section = ".user.text"]
static LBL_ENTRY_N: [u8; 8] = *b" entry #";

#[link_section = ".user.text"]
static LBL_UPTIME: [u8; 8] = *b" uptime=";

#[link_section = ".user.text"]
static LBL_MS: [u8; 2] = *b"ms";

#[link_section = ".user.text"]
static LBL_MC: [u8; 2] = *b"mC";

#[link_section = ".user.text"]
static LBL_WROTE: [u8; 7] = *b" wrote=";

#[link_section = ".user.text"]
static LBL_CONSOLE_ONLY: [u8; 15] = *b" (console only)";

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
        unsafe {
            core::ptr::write_volatile(buf.add(pos), b'0');
        }
        return pos + 1;
    }
    let mut tmp = [0u8; 20];
    let tp = tmp.as_mut_ptr();
    let mut n: usize = 0;
    let mut v = val;
    while v > 0 {
        unsafe {
            core::ptr::write_volatile(tp.add(n), b'0'.wrapping_add((v % 10) as u8));
        }
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

/// Write u64 as zero-padded 8-digit decimal (for uptime field).
#[link_section = ".user.text"]
#[inline(always)]
fn wu64_pad8(buf: *mut u8, pos: usize, val: u64) -> usize {
    let mut digits = [0u8; 8];
    let dp = digits.as_mut_ptr();
    let mut v = val;
    let mut i: usize = 8;
    while i > 0 {
        i = i.wrapping_sub(1);
        unsafe {
            core::ptr::write_volatile(dp.add(i), b'0'.wrapping_add((v % 10) as u8));
        }
        v /= 10;
    }
    let mut p = pos;
    i = 0;
    while i < 8 {
        unsafe {
            let c = core::ptr::read_volatile(dp.add(i));
            core::ptr::write_volatile(buf.add(p), c);
        }
        p = p.wrapping_add(1);
        i += 1;
    }
    p
}

#[link_section = ".user.text"]
#[inline(always)]
fn wi32(buf: *mut u8, pos: usize, val: i32) -> usize {
    if val < 0 {
        unsafe {
            core::ptr::write_volatile(buf.add(pos), b'-');
        }
        wu64(buf, pos + 1, (-(val as i64)) as u64)
    } else {
        wu64(buf, pos, val as u64)
    }
}

/// Check if a return value is an error sentinel.
#[link_section = ".user.text"]
#[inline(always)]
fn is_error(val: u64) -> bool {
    val >= E_NOSYS - 10
}

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn data_logger_main(_arg: usize) -> ! {
    // Print startup banner
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(1000);

    // Attempt to create log file
    sys_write_raw(MSG_CREATING.as_ptr(), MSG_CREATING.len());

    let fd_result = sys_fs_create(LOG_PATH.as_ptr(), LOG_PATH.len());
    let mut log_fd: u64 = u64::MAX;
    let mut file_ok: bool = false;

    if fd_result == E_PERM {
        // Capability denied — user tasks lack FS capability by default
        sys_write_raw(MSG_FAIL_PERM.as_ptr(), MSG_FAIL_PERM.len());
        sys_write_raw(MSG_FALLBACK.as_ptr(), MSG_FALLBACK.len());
    } else if is_error(fd_result) {
        // Other filesystem error
        sys_write_raw(MSG_FAIL_OTHER.as_ptr(), MSG_FAIL_OTHER.len());
        sys_write_raw(MSG_FALLBACK.as_ptr(), MSG_FALLBACK.len());
    } else {
        // Success — fd_result is the file descriptor
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
        sys_write_raw(MSG_LOGGING.as_ptr(), MSG_LOGGING.len());
        log_fd = fd_result;
        file_ok = true;
    }

    let mut entry_count: u64 = 0;

    loop {
        sys_delay(15000);

        entry_count = entry_count.wrapping_add(1);
        let uptime = sys_uptime();
        let temp = sys_temperature();

        // Format the log line: "[00001234] temp=45200 status=ok\n"
        let mut linebuf: core::mem::MaybeUninit<[u8; 64]> = core::mem::MaybeUninit::uninit();
        let lp = linebuf.as_mut_ptr() as *mut u8;
        let mut lpos: usize = 0;

        lpos = wstatic(lp, lpos, LBL_ENTRY.as_ptr(), LBL_ENTRY.len());
        lpos = wu64_pad8(lp, lpos, uptime);
        lpos = wstatic(lp, lpos, LBL_CLOSE.as_ptr(), LBL_CLOSE.len());
        lpos = wstatic(lp, lpos, LBL_TEMP.as_ptr(), LBL_TEMP.len());
        lpos = wi32(lp, lpos, temp);
        lpos = wstatic(lp, lpos, LBL_STATUS.as_ptr(), LBL_STATUS.len());
        lpos = wstatic(lp, lpos, S_NL.as_ptr(), S_NL.len());

        // Write to file if we have a valid fd
        let mut bytes_written: u64 = 0;
        if file_ok {
            let ret = sys_fs_write(log_fd, lp, lpos);
            if !is_error(ret) {
                bytes_written = ret;
            }
        }

        // Print console summary:
        //   "[data-logger] entry #1 uptime=12345ms temp=45200mC wrote=31 (console only)\n"
        let mut cbuf: core::mem::MaybeUninit<[u8; 128]> = core::mem::MaybeUninit::uninit();
        let cp = cbuf.as_mut_ptr() as *mut u8;
        let mut cpos: usize = 0;

        cpos = wstatic(cp, cpos, LBL_CONSOLE.as_ptr(), LBL_CONSOLE.len());
        // Skip the leading space in " entry #" to get "entry #"
        cpos = wstatic(
            cp,
            cpos,
            LBL_ENTRY_N.as_ptr().wrapping_add(1),
            LBL_ENTRY_N.len() - 1,
        );
        cpos = wu64(cp, cpos, entry_count);
        cpos = wstatic(cp, cpos, LBL_UPTIME.as_ptr(), LBL_UPTIME.len());
        cpos = wu64(cp, cpos, uptime);
        cpos = wstatic(cp, cpos, LBL_MS.as_ptr(), LBL_MS.len());
        unsafe {
            core::ptr::write_volatile(cp.add(cpos), b' ');
        }
        cpos = cpos.wrapping_add(1);
        cpos = wstatic(cp, cpos, LBL_TEMP.as_ptr(), LBL_TEMP.len());
        cpos = wi32(cp, cpos, temp);
        cpos = wstatic(cp, cpos, LBL_MC.as_ptr(), LBL_MC.len());

        if file_ok {
            cpos = wstatic(cp, cpos, LBL_WROTE.as_ptr(), LBL_WROTE.len());
            cpos = wu64(cp, cpos, bytes_written);
        } else {
            cpos = wstatic(cp, cpos, LBL_CONSOLE_ONLY.as_ptr(), LBL_CONSOLE_ONLY.len());
        }

        cpos = wstatic(cp, cpos, S_NL.as_ptr(), S_NL.len());
        sys_write_raw(cp, cpos);
    }
}
