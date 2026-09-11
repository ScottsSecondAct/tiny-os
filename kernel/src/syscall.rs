use arch::aarch64::exceptions::TrapFrame;
use crate::{kprintln, sched};

// --- Basic syscalls (direct) ---
const SYS_YIELD: u64 = 0;
const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_TASK_ID: u64 = 3;
const SYS_UPTIME: u64 = 4;
const SYS_EXIT: u64 = 5;
const SYS_TEMPERATURE: u64 = 6;

// --- Subsystem multiplexed syscalls ---
const SYS_FS: u64 = 10;
const SYS_NET: u64 = 11;
const SYS_SPI: u64 = 12;
const SYS_I2C: u64 = 13;
const SYS_GPIO: u64 = 14;

// --- Error codes (high bit set indicates error) ---
const E_NOSYS: u64 = u64::MAX;
const E_BADF: u64 = u64::MAX - 1;
const E_INVAL: u64 = u64::MAX - 2;
const E_NOMEM: u64 = u64::MAX - 3;
const E_IO: u64 = u64::MAX - 4;
const E_NOENT: u64 = u64::MAX - 5;
const E_NOSPC: u64 = u64::MAX - 6;
const E_BUSY: u64 = u64::MAX - 7;
const E_PERM: u64 = u64::MAX - 8;

// --- FS operation codes (X0) ---
const FS_OPEN: u64 = 0;
const FS_READ: u64 = 1;
const FS_WRITE: u64 = 2;
const FS_CLOSE: u64 = 3;
const FS_STAT: u64 = 4;
const FS_CREATE: u64 = 5;

// --- NET operation codes (X0) ---
const NET_SOCKET: u64 = 0;
const NET_BIND: u64 = 1;
const NET_CONNECT: u64 = 2;
const NET_SEND: u64 = 3;
const NET_RECV: u64 = 4;
const NET_CLOSE: u64 = 5;

// --- SPI operation codes (X0) ---
const SPI_OPEN: u64 = 0;
const SPI_TRANSFER: u64 = 1;
const SPI_CLOSE: u64 = 2;

// --- I2C operation codes (X0) ---
const I2C_OPEN: u64 = 0;
const I2C_READ: u64 = 1;
const I2C_WRITE: u64 = 2;
const I2C_CLOSE: u64 = 3;

// --- GPIO operation codes (X0) ---
const GPIO_SET_MODE: u64 = 0;
const GPIO_READ: u64 = 1;
const GPIO_WRITE: u64 = 2;
const GPIO_SET_PULL: u64 = 3;

use crate::os_cfg;

const MAX_USER_STR: usize = os_cfg::MAX_USER_STR;
const MAX_USER_BUF: usize = os_cfg::MAX_USER_BUF;

fn validate_user_buf(ptr: u64, len: usize) -> bool {
    !ptr.is_null_ptr() && len <= MAX_USER_BUF
}

trait IsNullPtr {
    fn is_null_ptr(&self) -> bool;
}

impl IsNullPtr for u64 {
    fn is_null_ptr(&self) -> bool {
        *self == 0
    }
}

unsafe fn user_str(ptr: u64, len: usize) -> Option<&'static str> {
    if ptr == 0 || len == 0 || len > MAX_USER_STR {
        return None;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(slice).ok()
}

unsafe fn user_slice(ptr: u64, len: usize) -> Option<&'static [u8]> {
    if ptr == 0 || len > MAX_USER_BUF {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(ptr as *const u8, len) })
}

unsafe fn user_slice_mut(ptr: u64, len: usize) -> Option<&'static mut [u8]> {
    if ptr == 0 || len > MAX_USER_BUF {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, len) })
}

// --- Filesystem syscall dispatch ---
fn dispatch_fs(op: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    use crate::fs;

    match op {
        FS_OPEN => {
            // a1=path_ptr, a2=path_len, a3=flags (0=read-only, 1=writable)
            let path = match unsafe { user_str(a1, a2 as usize) } {
                Some(p) => p,
                None => return E_INVAL,
            };
            let writable = a3 != 0;
            match fs::open(path, writable) {
                Ok(fd) => fd as u64,
                Err(_) => E_NOENT,
            }
        }
        FS_READ => {
            // a1=fd, a2=buf_ptr, a3=buf_len
            let buf = match unsafe { user_slice_mut(a2, a3 as usize) } {
                Some(b) => b,
                None => return E_INVAL,
            };
            match fs::read(a1 as usize, buf) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        FS_WRITE => {
            // a1=fd, a2=buf_ptr, a3=buf_len
            let buf = match unsafe { user_slice(a2, a3 as usize) } {
                Some(b) => b,
                None => return E_INVAL,
            };
            match fs::write(a1 as usize, buf) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        FS_CLOSE => {
            // a1=fd
            match fs::close(a1 as usize) {
                Ok(()) => 0,
                Err(_) => E_BADF,
            }
        }
        FS_STAT => {
            // a1=path_ptr, a2=path_len, a3=out_ptr (writes StatResult)
            let path = match unsafe { user_str(a1, a2 as usize) } {
                Some(p) => p,
                None => return E_INVAL,
            };
            if a3 == 0 {
                return E_INVAL;
            }
            match fs::stat(path) {
                Ok(entry) => {
                    let out = a3 as *mut StatResult;
                    unsafe {
                        core::ptr::write_unaligned(out, StatResult {
                            size: entry.size,
                            is_dir: if entry.is_dir { 1 } else { 0 },
                            cluster: entry.cluster,
                        });
                    }
                    0
                }
                Err(_) => E_NOENT,
            }
        }
        FS_CREATE => {
            // a1=path_ptr, a2=path_len
            let path = match unsafe { user_str(a1, a2 as usize) } {
                Some(p) => p,
                None => return E_INVAL,
            };
            match fs::create(path) {
                Ok(fd) => fd as u64,
                Err(_) => E_IO,
            }
        }
        _ => E_NOSYS,
    }
}

#[repr(C)]
pub struct StatResult {
    pub size: u32,
    pub is_dir: u32,
    pub cluster: u32,
}

// --- Network syscall dispatch ---
fn dispatch_net(op: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    use crate::net::socket;
    use crate::net::Ipv4Addr;

    match op {
        NET_SOCKET => {
            // a1=sock_type (0=UDP, 1=TCP)
            let sock_type = match a1 {
                0 => socket::SockType::Udp,
                1 => socket::SockType::Tcp,
                _ => return E_INVAL,
            };
            match socket::socket(sock_type) {
                Ok(fd) => fd as u64,
                Err(_) => E_NOMEM,
            }
        }
        NET_BIND => {
            // a1=fd, a2=port
            match socket::bind(a1 as u8, a2 as u16) {
                Ok(()) => 0,
                Err(_) => E_BUSY,
            }
        }
        NET_CONNECT => {
            // a1=fd, a2=ipv4_addr (packed as u32), a3=port
            let octets = (a2 as u32).to_be_bytes();
            let addr = Ipv4Addr(octets);
            match socket::connect(a1 as u8, addr, a3 as u16) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        NET_SEND => {
            // a1=fd, a2=data_ptr, a3=data_len
            let data = match unsafe { user_slice(a2, a3 as usize) } {
                Some(d) => d,
                None => return E_INVAL,
            };
            match socket::send(a1 as u8, data) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        NET_RECV => {
            // a1=fd, a2=buf_ptr, a3=buf_len
            let buf = match unsafe { user_slice_mut(a2, a3 as usize) } {
                Some(b) => b,
                None => return E_INVAL,
            };
            match socket::recvfrom(a1 as u8, buf) {
                Ok((n, _addr, _port)) => n as u64,
                Err(_) => E_IO,
            }
        }
        NET_CLOSE => {
            // a1=fd
            socket::close(a1 as u8);
            0
        }
        _ => E_NOSYS,
    }
}

// --- SPI syscall dispatch ---
fn dispatch_spi(op: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    use crate::periph;
    use arch::spi::{SpiConfig, SpiMode};

    match op {
        SPI_OPEN => {
            // a1=clock_hz, a2=mode (0-3), a3=cs_pin
            let mode = match a2 {
                0 => SpiMode::Mode0,
                1 => SpiMode::Mode1,
                2 => SpiMode::Mode2,
                3 => SpiMode::Mode3,
                _ => return E_INVAL,
            };
            let config = SpiConfig {
                clock_hz: a1 as u32,
                mode,
                bits_per_word: 8,
                cs_pin: a3 as u8,
            };
            match periph::spi_configure(&config) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        SPI_TRANSFER => {
            // a1=tx_ptr, a2=rx_ptr, a3=len
            let len = a3 as usize;
            let tx = match unsafe { user_slice(a1, len) } {
                Some(s) => s,
                None => return E_INVAL,
            };
            let rx = match unsafe { user_slice_mut(a2, len) } {
                Some(s) => s,
                None => return E_INVAL,
            };
            match periph::spi_transfer(tx, rx) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        SPI_CLOSE => {
            0
        }
        _ => E_NOSYS,
    }
}

// --- I2C syscall dispatch ---
fn dispatch_i2c(op: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    use crate::periph;
    use arch::i2c::I2cConfig;

    match op {
        I2C_OPEN => {
            // a1=clock_hz (100000 or 400000)
            let config = I2cConfig { clock_hz: a1 as u32 };
            match periph::i2c_configure(&config) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        I2C_READ => {
            // a1=device_addr, a2=buf_ptr, a3=buf_len
            let buf = match unsafe { user_slice_mut(a2, a3 as usize) } {
                Some(b) => b,
                None => return E_INVAL,
            };
            match periph::i2c_read(a1 as u8, buf) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        I2C_WRITE => {
            // a1=device_addr, a2=buf_ptr, a3=buf_len
            let buf = match unsafe { user_slice(a2, a3 as usize) } {
                Some(b) => b,
                None => return E_INVAL,
            };
            match periph::i2c_write(a1 as u8, buf) {
                Ok(n) => n as u64,
                Err(_) => E_IO,
            }
        }
        I2C_CLOSE => {
            0
        }
        _ => E_NOSYS,
    }
}

// --- GPIO syscall dispatch ---
fn dispatch_gpio(op: u64, a1: u64, a2: u64, _a3: u64) -> u64 {
    use crate::periph;
    use arch::gpio::{PinMode, PullMode};

    match op {
        GPIO_SET_MODE => {
            // a1=pin, a2=mode (0=Input, 1=Output, 2-7=AltFunc0-5)
            let mode = match a2 {
                0 => PinMode::Input,
                1 => PinMode::Output,
                2 => PinMode::AltFunc0,
                3 => PinMode::AltFunc1,
                4 => PinMode::AltFunc2,
                5 => PinMode::AltFunc3,
                6 => PinMode::AltFunc4,
                7 => PinMode::AltFunc5,
                _ => return E_INVAL,
            };
            match periph::gpio_set_mode(a1 as u8, mode) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        GPIO_READ => {
            // a1=pin
            match periph::gpio_read(a1 as u8) {
                Ok(high) => if high { 1 } else { 0 },
                Err(_) => E_IO,
            }
        }
        GPIO_WRITE => {
            // a1=pin, a2=value (0=low, 1=high)
            match periph::gpio_write(a1 as u8, a2 != 0) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        GPIO_SET_PULL => {
            // a1=pin, a2=pull (0=None, 1=Up, 2=Down)
            let pull = match a2 {
                0 => PullMode::None,
                1 => PullMode::Up,
                2 => PullMode::Down,
                _ => return E_INVAL,
            };
            match periph::gpio_set_pull(a1 as u8, pull) {
                Ok(()) => 0,
                Err(_) => E_IO,
            }
        }
        _ => E_NOSYS,
    }
}

fn cap_for_syscall(nr: u64) -> u32 {
    match nr {
        SYS_YIELD => sched::CAP_YIELD,
        SYS_DELAY => sched::CAP_DELAY,
        SYS_WRITE => sched::CAP_WRITE,
        SYS_TASK_ID => sched::CAP_TASKID,
        SYS_UPTIME => sched::CAP_UPTIME,
        SYS_EXIT => sched::CAP_EXIT,
        SYS_TEMPERATURE => sched::CAP_TEMP,
        SYS_FS => sched::CAP_FS,
        SYS_NET => sched::CAP_NET,
        SYS_SPI => sched::CAP_SPI,
        SYS_I2C => sched::CAP_I2C,
        SYS_GPIO => sched::CAP_GPIO,
        _ => 0,
    }
}

pub fn dispatch(tf: &mut TrapFrame) {
    let syscall_nr = tf.regs[8];
    let a0 = tf.regs[0];
    let a1 = tf.regs[1];
    let a2 = tf.regs[2];
    let a3 = tf.regs[3];

    let required_cap = cap_for_syscall(syscall_nr);
    if required_cap != 0 && !sched::task_has_capability(required_cap) {
        crate::audit::log(crate::audit::AuditEvent::CapabilityDenied,
            match syscall_nr {
                SYS_FS => "FS", SYS_NET => "NET", SYS_SPI => "SPI",
                SYS_I2C => "I2C", SYS_GPIO => "GPIO",
                _ => "syscall",
            });
        tf.regs[0] = E_PERM;
        return;
    }

    let result: u64 = match syscall_nr {
        SYS_YIELD => {
            sched::task_yield();
            0
        }
        SYS_DELAY => {
            sched::delay(a0 as u32);
            0
        }
        SYS_WRITE => {
            let ptr = a0 as *const u8;
            let len = a1 as usize;
            if len <= 256 && !ptr.is_null() {
                let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(s) = core::str::from_utf8(slice) {
                    crate::print::_print_str(s);
                }
            }
            len as u64
        }
        SYS_TASK_ID => {
            sched::current_task_id() as u64
        }
        SYS_UPTIME => {
            arch::aarch64::exceptions::tick_count()
        }
        SYS_EXIT => {
            let id = sched::current_task_id();
            kprintln!("[syscall] task {} called exit", id);
            sched::task_terminate(id);
            0
        }
        SYS_TEMPERATURE => {
            match arch::aarch64::mailbox::get_temperature() {
                Some(mc) => mc as u64,
                None => u64::MAX,
            }
        }
        SYS_FS => dispatch_fs(a0, a1, a2, a3),
        SYS_NET => dispatch_net(a0, a1, a2, a3),
        SYS_SPI => dispatch_spi(a0, a1, a2, a3),
        SYS_I2C => dispatch_i2c(a0, a1, a2, a3),
        SYS_GPIO => dispatch_gpio(a0, a1, a2, a3),
        _ => {
            kprintln!("[syscall] unknown syscall {}", syscall_nr);
            E_NOSYS
        }
    };

    tf.regs[0] = result;
}
