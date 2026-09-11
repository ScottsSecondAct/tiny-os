//! UDP Echo Server — user-space application for tiny_os
//!
//! Runs at EL0. Creates a UDP socket, binds to port 7777, and echoes received
//! packets back to the sender. Demonstrates the full network syscall lifecycle:
//! socket creation, binding, polling receive, and send. Prints periodic status
//! updates with packet counts and uptime.
//!
//! On QEMU the loopback device is active so the echo server is fully
//! functional, though no external traffic arrives unless injected.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_YIELD: u64 = 0;
const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_NET: u64 = 11;

// NET operations
const NET_SOCKET: u64 = 0;
const NET_BIND: u64 = 1;
const NET_SEND: u64 = 3;
const NET_RECV: u64 = 4;
const NET_CLOSE: u64 = 5;

const E_NOSYS: u64 = u64::MAX;
const E_PERM: u64 = u64::MAX - 8;

const LISTEN_PORT: u16 = 7777;
const STATUS_INTERVAL_MS: u64 = 30_000;

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
fn sys_yield() { syscall(SYS_YIELD, 0, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_delay(ms: u32) { syscall(SYS_DELAY, ms as u64, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall(SYS_WRITE, ptr as u64, len as u64); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 { syscall(SYS_UPTIME, 0, 0) }

// NET syscall wrappers

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_socket(sock_type: u64) -> u64 {
    syscall4(SYS_NET, NET_SOCKET, sock_type, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_bind(fd: u64, port: u16) -> u64 {
    syscall4(SYS_NET, NET_BIND, fd, port as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_recv(fd: u64, buf: *mut u8, len: usize) -> u64 {
    syscall4(SYS_NET, NET_RECV, fd, buf as u64, len as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_send(fd: u64, buf: *const u8, len: usize) -> u64 {
    syscall4(SYS_NET, NET_SEND, fd, buf as u64, len as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_close(fd: u64) {
    syscall4(SYS_NET, NET_CLOSE, fd, 0, 0);
}

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 33] = *b"[echo-srv] UDP echo server (EL0)\n";

#[link_section = ".user.text"]
static MSG_SOCK_OK: [u8; 26] = *b"[echo-srv] socket created\n";

#[link_section = ".user.text"]
static MSG_SOCK_FAIL: [u8; 32] = *b"[echo-srv] socket create failed\n";

#[link_section = ".user.text"]
static MSG_BIND_OK: [u8; 26] = *b"[echo-srv] bind succeeded\n";

#[link_section = ".user.text"]
static MSG_BIND_FAIL: [u8; 23] = *b"[echo-srv] bind failed\n";

#[link_section = ".user.text"]
static MSG_LISTEN: [u8; 30] = *b"[echo-srv] listening for data\n";

#[link_section = ".user.text"]
static MSG_PERM: [u8; 35] = *b"[echo-srv] permission denied (net)\n";

#[link_section = ".user.text"]
static LBL: [u8; 11] = *b"[echo-srv] ";

#[link_section = ".user.text"]
static S_ECHO: [u8; 7] = *b"echoed ";

#[link_section = ".user.text"]
static S_BYTES: [u8; 6] = *b" bytes";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static S_STATUS: [u8; 8] = *b"status: ";

#[link_section = ".user.text"]
static S_PKTS: [u8; 5] = *b" pkts";

#[link_section = ".user.text"]
static S_UP: [u8; 4] = *b" up=";

#[link_section = ".user.text"]
static S_MS: [u8; 2] = *b"ms";

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

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn echo_server_main(_arg: usize) -> ! {
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // Create a UDP socket (type 0 = UDP)
    let sock_fd = sys_net_socket(0);

    if sock_fd == E_PERM {
        sys_write_raw(MSG_PERM.as_ptr(), MSG_PERM.len());
        loop { sys_yield(); }
    }

    if sock_fd >= E_NOSYS - 10 {
        sys_write_raw(MSG_SOCK_FAIL.as_ptr(), MSG_SOCK_FAIL.len());
        loop { sys_yield(); }
    }

    sys_write_raw(MSG_SOCK_OK.as_ptr(), MSG_SOCK_OK.len());

    // Bind to port 7777
    let bind_ret = sys_net_bind(sock_fd, LISTEN_PORT);

    if bind_ret != 0 {
        sys_write_raw(MSG_BIND_FAIL.as_ptr(), MSG_BIND_FAIL.len());
        sys_net_close(sock_fd);
        loop { sys_yield(); }
    }

    sys_write_raw(MSG_BIND_OK.as_ptr(), MSG_BIND_OK.len());
    sys_write_raw(MSG_LISTEN.as_ptr(), MSG_LISTEN.len());

    let mut packets_echoed: u64 = 0;
    let mut last_status_time: u64 = sys_uptime();

    // Receive buffer — 512 bytes on the stack
    let mut rxbuf: core::mem::MaybeUninit<[u8; 512]> = core::mem::MaybeUninit::uninit();
    let rxptr = rxbuf.as_mut_ptr() as *mut u8;

    loop {
        // Poll for incoming data
        let n = sys_net_recv(sock_fd, rxptr, 512);

        if n > 0 && n < E_NOSYS - 10 {
            // Echo the received data back
            let sent = sys_net_send(sock_fd, rxptr as *const u8, n as usize);
            let _ = sent;

            packets_echoed = packets_echoed.wrapping_add(1);

            // Print: "[echo-srv] echoed <N> bytes\n"
            let mut outbuf: core::mem::MaybeUninit<[u8; 64]> = core::mem::MaybeUninit::uninit();
            let p = outbuf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;
            pos = wstatic(p, pos, LBL.as_ptr(), LBL.len());
            pos = wstatic(p, pos, S_ECHO.as_ptr(), S_ECHO.len());
            pos = wu64(p, pos, n);
            pos = wstatic(p, pos, S_BYTES.as_ptr(), S_BYTES.len());
            pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());
            sys_write_raw(p, pos);
        } else {
            // No data available — yield to other tasks
            sys_yield();
        }

        // Periodic status report every 30 seconds
        let now = sys_uptime();
        if now.wrapping_sub(last_status_time) >= STATUS_INTERVAL_MS {
            last_status_time = now;

            // Print: "[echo-srv] status: <count> pkts up=<uptime>ms\n"
            let mut sbuf: core::mem::MaybeUninit<[u8; 80]> = core::mem::MaybeUninit::uninit();
            let p = sbuf.as_mut_ptr() as *mut u8;
            let mut pos: usize = 0;
            pos = wstatic(p, pos, LBL.as_ptr(), LBL.len());
            pos = wstatic(p, pos, S_STATUS.as_ptr(), S_STATUS.len());
            pos = wu64(p, pos, packets_echoed);
            pos = wstatic(p, pos, S_PKTS.as_ptr(), S_PKTS.len());
            pos = wstatic(p, pos, S_UP.as_ptr(), S_UP.len());
            pos = wu64(p, pos, now);
            pos = wstatic(p, pos, S_MS.as_ptr(), S_MS.len());
            pos = wstatic(p, pos, S_NL.as_ptr(), S_NL.len());
            sys_write_raw(p, pos);
        }

        // Small delay to avoid busy-spinning
        sys_delay(10);
    }
}
