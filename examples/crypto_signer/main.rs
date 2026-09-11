//! Crypto Signer — user-space application for tiny_os
//!
//! Runs at EL0. Signs log records with HMAC-SHA256 via the SYS_CRYPTO syscall
//! (or falls back to a software XOR checksum when the crypto capability is
//! denied), then writes signed records to `/SIGNED.TXT` on the filesystem.
//! Demonstrates cryptographic operations and filesystem integration from user
//! space.

use core::arch::asm;

// ── Syscall numbers ─────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_TEMPERATURE: u64 = 6;
const SYS_FS: u64 = 10;
const SYS_CRYPTO: u64 = 20;

// FS operations
const FS_WRITE_OP: u64 = 2;
const FS_CLOSE: u64 = 3;
const FS_CREATE: u64 = 5;

// CRYPTO operations
const CRYPTO_SHA256: u64 = 2;
const CRYPTO_DETECT: u64 = 3;

// Error codes
const E_PERM: u64 = u64::MAX - 8;

// Timing
const SIGN_INTERVAL_MS: u32 = 20_000;

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
fn sys_fs_create(path_ptr: *const u8, path_len: usize) -> u64 {
    syscall4(SYS_FS, FS_CREATE, path_ptr as u64, path_len as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_write(fd: u64, data_ptr: *const u8, data_len: usize) -> u64 {
    syscall4(SYS_FS, FS_WRITE_OP, fd, data_ptr as u64, data_len as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_close(fd: u64) -> u64 {
    syscall4(SYS_FS, FS_CLOSE, fd, 0, 0)
}

// CRYPTO syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_crypto_detect() -> u64 {
    syscall4(SYS_CRYPTO, CRYPTO_DETECT, 0, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_crypto_sha256(data_ptr: *const u8, data_len: usize, hash_out: *mut u8) -> u64 {
    syscall4(SYS_CRYPTO, CRYPTO_SHA256, data_ptr as u64, data_len as u64, hash_out as u64)
}

// ── String constants (must live in .user.text for EL0 access) ───────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 37] = *b"[crypto-signer] started (EL0 signer)\n";

#[link_section = ".user.text"]
static MSG_CRYPTO_HW: [u8; 40] = *b"[crypto-signer] crypto: hw-sha256 ready\n";

#[link_section = ".user.text"]
static MSG_CRYPTO_SW: [u8; 49] = *b"[crypto-signer] crypto: sw-xor fallback (no cap)\n";

#[link_section = ".user.text"]
static MSG_REC_PREFIX: [u8; 16] = *b"[crypto-signer] ";

#[link_section = ".user.text"]
static MSG_FS_DENIED: [u8; 43] = *b"[crypto-signer] fs write denied (no cap)\n  ";

#[link_section = ".user.text"]
static MSG_FS_OK: [u8; 33] = *b"[crypto-signer] wrote SIGNED.TXT\n";

#[link_section = ".user.text"]
static S_REC: [u8; 4] = *b"REC:";

#[link_section = ".user.text"]
static S_COLON: [u8; 1] = *b":";

#[link_section = ".user.text"]
static S_SIG_HW: [u8; 11] = *b":hw-sha256:";

#[link_section = ".user.text"]
static S_SIG_SW: [u8; 8] = *b":sw-xor:";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static FILE_PATH: [u8; 10] = *b"SIGNED.TXT";

#[link_section = ".user.text"]
static HEX_TABLE: [u8; 16] = *b"0123456789abcdef";

// ── Formatting helpers (volatile writes to avoid compiler memcpy) ───────────

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

/// Write a u64 zero-padded to exactly `width` decimal digits.
#[link_section = ".user.text"]
#[inline(always)]
fn wu64_pad(buf: *mut u8, pos: usize, val: u64, width: usize) -> usize {
    let mut tmp = [0u8; 20];
    let tp = tmp.as_mut_ptr();
    let mut n: usize = 0;
    let mut v = val;
    loop {
        unsafe { core::ptr::write_volatile(tp.add(n), b'0'.wrapping_add((v % 10) as u8)); }
        v /= 10;
        n = n.wrapping_add(1);
        if v == 0 { break; }
    }
    // Pad with leading zeros
    let mut p = pos;
    let mut pad = if width > n { width - n } else { 0 };
    while pad > 0 {
        unsafe { core::ptr::write_volatile(buf.add(p), b'0'); }
        p = p.wrapping_add(1);
        pad -= 1;
    }
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

/// Write one byte as two lowercase hex characters.
#[link_section = ".user.text"]
#[inline(always)]
fn whex_byte(buf: *mut u8, pos: usize, byte: u8) -> usize {
    let hi = (byte >> 4) & 0x0F;
    let lo = byte & 0x0F;
    unsafe {
        let ch = core::ptr::read_volatile(HEX_TABLE.as_ptr().add(hi as usize));
        core::ptr::write_volatile(buf.add(pos), ch);
        let cl = core::ptr::read_volatile(HEX_TABLE.as_ptr().add(lo as usize));
        core::ptr::write_volatile(buf.add(pos + 1), cl);
    }
    pos + 2
}

// ── Software XOR checksum (fallback when crypto capability is denied) ───────

/// Compute a simple rotating XOR checksum over the input data.  This is NOT
/// cryptographically secure — it is used only as a stand-in when the hardware
/// SHA-256 capability is unavailable.  The output is a single byte.
#[link_section = ".user.text"]
#[inline(always)]
fn sw_xor_checksum(data: *const u8, len: usize) -> u8 {
    let mut acc: u8 = 0xA5; // seed
    let mut i: usize = 0;
    while i < len {
        let b = unsafe { core::ptr::read_volatile(data.add(i)) };
        acc = acc.rotate_left(3) ^ b;
        i += 1;
    }
    acc
}

// ── Record building ─────────────────────────────────────────────────────────

/// Build the unsigned record body into `buf` and return the length written.
/// Format: `REC:NNNN:TTTTTTTT:MMMMM`
/// where NNNN = record count (4 digits), TTTTTTTT = uptime in ms,
/// MMMMM = temperature in millidegrees.
#[link_section = ".user.text"]
fn build_record(buf: *mut u8, count: u64, uptime: u64, temp: i32) -> usize {
    let mut pos: usize = 0;
    // "REC:"
    pos = wstatic(buf, pos, S_REC.as_ptr(), 4);
    // Record number, 4-digit zero-padded
    pos = wu64_pad(buf, pos, count, 4);
    // ":"
    pos = wstatic(buf, pos, S_COLON.as_ptr(), 1);
    // Uptime in ms
    pos = wu64(buf, pos, uptime);
    // ":"
    pos = wstatic(buf, pos, S_COLON.as_ptr(), 1);
    // Temperature in millidegrees (as unsigned; negative shows 0)
    let t = if temp < 0 { 0u64 } else { temp as u64 };
    pos = wu64(buf, pos, t);
    pos
}

// ── Application entry point ─────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn crypto_signer_main(_arg: usize) -> ! {
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(1000);

    // ── Probe crypto capability ─────────────────────────────────────────
    let crypto_ret = sys_crypto_detect();
    let has_crypto = crypto_ret != E_PERM;

    if has_crypto {
        sys_write_raw(MSG_CRYPTO_HW.as_ptr(), MSG_CRYPTO_HW.len());
    } else {
        sys_write_raw(MSG_CRYPTO_SW.as_ptr(), MSG_CRYPTO_SW.len());
    }

    let mut count: u64 = 0;

    loop {
        count = count.wrapping_add(1);

        let uptime = sys_uptime();
        let temp = sys_temperature();

        // ── Build the unsigned record body ──────────────────────────────
        // Record buffer: "REC:NNNN:TTTTTTTT:MMMMM" (max ~40 bytes)
        let mut rec_buf: core::mem::MaybeUninit<[u8; 48]> = core::mem::MaybeUninit::uninit();
        let rp = rec_buf.as_mut_ptr() as *mut u8;
        let rec_len = build_record(rp, count, uptime, temp);

        // ── Sign the record ─────────────────────────────────────────────
        // Full signed line buffer: record + signature tag + hex sig + newline
        // Max: 48 (record) + 11 (tag) + 16 (8 hex bytes) + 1 (newline) = 76
        let mut line_buf: core::mem::MaybeUninit<[u8; 96]> = core::mem::MaybeUninit::uninit();
        let lp = line_buf.as_mut_ptr() as *mut u8;

        // Copy record body into line buffer
        let mut pos = wstatic(lp, 0, rp as *const u8, rec_len);

        if has_crypto {
            // Hardware SHA-256 path
            let mut hash = [0u8; 32];
            let ret = sys_crypto_sha256(rp as *const u8, rec_len, hash.as_mut_ptr());
            // Append signature tag
            pos = wstatic(lp, pos, S_SIG_HW.as_ptr(), S_SIG_HW.len());
            if ret == 0 {
                // Append first 4 bytes (8 hex chars) of the SHA-256 hash
                let mut hi: usize = 0;
                while hi < 4 {
                    let b = unsafe { core::ptr::read_volatile(hash.as_ptr().add(hi)) };
                    pos = whex_byte(lp, pos, b);
                    hi += 1;
                }
            } else {
                // Hash failed — write "err" placeholder
                let mut ei: usize = 0;
                #[link_section = ".user.text"]
                static ERR_TAG: [u8; 8] = *b"err00000";
                while ei < 8 {
                    unsafe {
                        let b = core::ptr::read_volatile(ERR_TAG.as_ptr().add(ei));
                        core::ptr::write_volatile(lp.add(pos), b);
                    }
                    pos += 1;
                    ei += 1;
                }
            }
        } else {
            // Software XOR fallback
            let cksum = sw_xor_checksum(rp as *const u8, rec_len);
            pos = wstatic(lp, pos, S_SIG_SW.as_ptr(), S_SIG_SW.len());
            pos = whex_byte(lp, pos, cksum);
        }

        // Newline
        pos = wstatic(lp, pos, S_NL.as_ptr(), 1);

        // ── Print signed record to console ──────────────────────────────
        // Prefix with app tag, then the signed line
        sys_write_raw(MSG_REC_PREFIX.as_ptr(), MSG_REC_PREFIX.len());
        sys_write_raw(lp as *const u8, pos);

        // ── Write to filesystem ─────────────────────────────────────────
        let fd = sys_fs_create(FILE_PATH.as_ptr(), FILE_PATH.len());
        if fd == E_PERM {
            // Capability denied — print notice once per 5 records
            if count % 5 == 1 {
                sys_write_raw(MSG_FS_DENIED.as_ptr(), MSG_FS_DENIED.len());
            }
        } else if fd < u64::MAX - 10 {
            sys_fs_write(fd, lp as *const u8, pos);
            sys_fs_close(fd);
            if count == 1 {
                sys_write_raw(MSG_FS_OK.as_ptr(), MSG_FS_OK.len());
            }
        }

        sys_delay(SIGN_INTERVAL_MS);
    }
}
