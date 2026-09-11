//! Industrial Sensor Gateway — user-space application for tiny_os
//!
//! Runs at EL0 on a Raspberry Pi 5. Collects sensor data over SPI (pressure),
//! I2C (temperature/humidity), and GPIO (digital vibration switch), logs
//! readings to the SD card via the filesystem syscalls, and forwards them over
//! UDP to a monitoring host.
//!
//! Scheduling: runs at priority 80 (high) for deterministic sensor sampling.
//! The network forwarding and SD logging are lower-latency operations that
//! happen inline after each sample batch.
//!
//! All hardware access is mediated through kernel syscalls — no direct MMIO.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_TEMPERATURE: u64 = 6;
const SYS_FS: u64 = 10;
const SYS_NET: u64 = 11;
const SYS_SPI: u64 = 12;
const SYS_I2C: u64 = 13;
const SYS_GPIO: u64 = 14;

// FS operations
const FS_CREATE: u64 = 5;
const FS_WRITE_OP: u64 = 2;
const FS_CLOSE: u64 = 3;

// NET operations
const NET_SOCKET: u64 = 0;
const NET_SEND: u64 = 3;
const NET_CLOSE: u64 = 5;

// SPI operations
const SPI_OPEN: u64 = 0;
const SPI_TRANSFER: u64 = 1;

// I2C operations
const I2C_OPEN: u64 = 0;
const I2C_WRITE_OP: u64 = 2;
const I2C_READ: u64 = 1;

// GPIO operations
const GPIO_SET_MODE: u64 = 0;
const GPIO_READ: u64 = 1;

const E_NOSYS: u64 = u64::MAX;

// ── Sensor addresses and config ──────────────────────────────────────────────

const BMP280_I2C_ADDR: u8 = 0x76;
const VIBRATION_GPIO_PIN: u8 = 17;
const SPI_CLOCK_HZ: u32 = 1_000_000;

const SAMPLE_INTERVAL_MS: u32 = 1000;
const LOG_BATCH_SIZE: usize = 10;
const MONITOR_PORT: u16 = 5000;

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
fn sys_fs_create(path: &[u8]) -> u64 {
    syscall4(SYS_FS, FS_CREATE, path.as_ptr() as u64, path.len() as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_fs_write(fd: u64, data: &[u8]) -> u64 {
    syscall4(SYS_FS, FS_WRITE_OP, fd, data.as_ptr() as u64, data.len() as u64)
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
fn sys_net_send(fd: u64, data: &[u8]) -> u64 {
    syscall4(SYS_NET, NET_SEND, fd, data.as_ptr() as u64, data.len() as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_net_close(fd: u64) {
    syscall4(SYS_NET, NET_CLOSE, fd, 0, 0);
}

// SPI syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_spi_open(clock_hz: u32, mode: u8, cs: u8) -> u64 {
    syscall4(SYS_SPI, SPI_OPEN, clock_hz as u64, mode as u64, cs as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_spi_transfer(tx: &[u8], rx: &mut [u8]) -> u64 {
    let len = tx.len().min(rx.len());
    syscall4(SYS_SPI, SPI_TRANSFER, tx.as_ptr() as u64, rx.as_mut_ptr() as u64, len as u64)
}

// I2C syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_i2c_open(clock_hz: u32) -> u64 {
    syscall4(SYS_I2C, I2C_OPEN, clock_hz as u64, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_i2c_write(addr: u8, data: &[u8]) -> u64 {
    syscall4(SYS_I2C, I2C_WRITE_OP, addr as u64, data.as_ptr() as u64, data.len() as u64)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_i2c_read(addr: u8, buf: &mut [u8]) -> u64 {
    syscall4(SYS_I2C, I2C_READ, addr as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
}

// GPIO syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_set_mode(pin: u8, mode: u8) -> u64 {
    syscall4(SYS_GPIO, GPIO_SET_MODE, pin as u64, mode as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_read(pin: u8) -> u64 {
    syscall4(SYS_GPIO, GPIO_READ, pin as u64, 0, 0)
}

// ── String constants ─────────────────────────────────────────────────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 42] = *b"[gateway] industrial sensor gateway (EL0)\n";

#[link_section = ".user.text"]
static MSG_INIT_SPI: [u8; 25] = *b"[gateway] init SPI bus.. ";

#[link_section = ".user.text"]
static MSG_INIT_I2C: [u8; 25] = *b"[gateway] init I2C bus.. ";

#[link_section = ".user.text"]
static MSG_INIT_GPIO: [u8; 25] = *b"[gateway] init GPIO..    ";

#[link_section = ".user.text"]
static MSG_INIT_NET: [u8; 25] = *b"[gateway] init network.. ";

#[link_section = ".user.text"]
static MSG_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_STUB: [u8; 13] = *b"stub (no hw)\n";

#[link_section = ".user.text"]
static MSG_FAIL: [u8; 5] = *b"fail\n";

#[link_section = ".user.text"]
static MSG_RUNNING: [u8; 27] = *b"[gateway] sampling started\n";

#[link_section = ".user.text"]
static MSG_SAMPLE: [u8; 10] = *b"[gateway] ";

#[link_section = ".user.text"]
static S_TEMP: [u8; 2] = *b"T=";

#[link_section = ".user.text"]
static S_PRESS: [u8; 3] = *b" P=";

#[link_section = ".user.text"]
static S_VIB: [u8; 3] = *b" V=";

#[link_section = ".user.text"]
static S_HPA: [u8; 4] = *b"hPa ";

#[link_section = ".user.text"]
static S_MC: [u8; 3] = *b"mC ";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

#[link_section = ".user.text"]
static S_TICK: [u8; 2] = *b"t=";

#[link_section = ".user.text"]
static S_COMMA: [u8; 1] = *b",";

#[link_section = ".user.text"]
static LOG_PATH: [u8; 10] = *b"sensor.log";

// ── Formatting helpers ───────────────────────────────────────────────────────

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

#[link_section = ".user.text"]
#[inline(always)]
fn wi32(buf: *mut u8, pos: usize, val: i32) -> usize {
    if val < 0 {
        unsafe { core::ptr::write_volatile(buf.add(pos), b'-'); }
        wu64(buf, pos + 1, (-(val as i64)) as u64)
    } else {
        wu64(buf, pos, val as u64)
    }
}

// ── Sensor data structure ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct SensorReading {
    timestamp: u64,
    temperature_mc: i32,
    pressure_hpa: u32,
    vibration: bool,
}

// ── Sensor reading functions ─────────────────────────────────────────────────

#[link_section = ".user.text"]
fn read_pressure_spi(spi_ok: bool) -> u32 {
    if !spi_ok {
        return 101325; // 1013.25 hPa default (standard atmosphere)
    }
    // Read pressure register from SPI sensor (e.g., BMP280 in SPI mode)
    // Command: read register 0xF7-0xF9 (pressure data)
    let tx = [0xF7 | 0x80, 0x00, 0x00, 0x00]; // Read bit + register addr + 3 dummy
    let mut rx = [0u8; 4];
    let ret = sys_spi_transfer(&tx, &mut rx);
    if ret >= E_NOSYS - 10 {
        return 101325;
    }
    // BMP280 raw pressure is 20-bit in rx[1..4]
    let raw = ((rx[1] as u32) << 12) | ((rx[2] as u32) << 4) | ((rx[3] as u32) >> 4);
    // Simplified conversion (no compensation — would need calibration data)
    // Return as Pa (approximately)
    if raw > 0 { raw / 4 } else { 101325 }
}

#[link_section = ".user.text"]
fn read_temperature_i2c(i2c_ok: bool) -> i32 {
    if !i2c_ok {
        return sys_temperature(); // Fall back to SoC temperature
    }
    // Read temperature from I2C sensor (e.g., BMP280 at 0x76)
    // Write register address 0xFA (temp MSB), then read 3 bytes
    let reg = [0xFA_u8];
    let wr = sys_i2c_write(BMP280_I2C_ADDR, &reg);
    if wr >= E_NOSYS - 10 {
        return sys_temperature();
    }
    let mut data = [0u8; 3];
    let rd = sys_i2c_read(BMP280_I2C_ADDR, &mut data);
    if rd >= E_NOSYS - 10 {
        return sys_temperature();
    }
    // BMP280 raw temperature is 20-bit
    let raw = ((data[0] as i32) << 12) | ((data[1] as i32) << 4) | ((data[2] as i32) >> 4);
    // Simplified conversion to millidegrees Celsius
    // (Real implementation would use BMP280 compensation formula)
    if raw > 0 { raw * 10 / 52 } else { sys_temperature() }
}

#[link_section = ".user.text"]
fn read_vibration_gpio(gpio_ok: bool) -> bool {
    if !gpio_ok {
        return false;
    }
    let val = sys_gpio_read(VIBRATION_GPIO_PIN);
    val == 1
}

// ── Logging ──────────────────────────────────────────────────────────────────

#[link_section = ".user.text"]
fn log_reading(reading: &SensorReading) {
    // Format: "t=<tick>,T=<temp>,P=<press>,V=<0|1>\n"
    let mut linebuf: core::mem::MaybeUninit<[u8; 64]> = core::mem::MaybeUninit::uninit();
    let p = linebuf.as_mut_ptr() as *mut u8;
    let mut pos: usize = 0;

    pos = wstatic(p, pos, S_TICK.as_ptr(), 2);
    pos = wu64(p, pos, reading.timestamp);
    pos = wstatic(p, pos, S_COMMA.as_ptr(), 1);
    pos = wstatic(p, pos, S_TEMP.as_ptr(), 2);
    pos = wi32(p, pos, reading.temperature_mc);
    pos = wstatic(p, pos, S_COMMA.as_ptr(), 1);
    pos = wstatic(p, pos, S_PRESS.as_ptr().wrapping_add(1), 2); // "P=" without leading space
    pos = wu64(p, pos, reading.pressure_hpa as u64);
    pos = wstatic(p, pos, S_COMMA.as_ptr(), 1);
    pos = wstatic(p, pos, S_VIB.as_ptr().wrapping_add(1), 2); // "V=" without leading space
    pos = wu64(p, pos, if reading.vibration { 1 } else { 0 });
    pos = wstatic(p, pos, S_NL.as_ptr(), 1);

    // Write to log file
    let fd = sys_fs_create(&LOG_PATH);
    if fd < E_NOSYS - 10 {
        let line = unsafe { core::slice::from_raw_parts(p, pos) };
        sys_fs_write(fd, line);
        sys_fs_close(fd);
    }
}

#[link_section = ".user.text"]
fn send_reading_udp(sock_fd: u64, reading: &SensorReading) {
    if sock_fd >= E_NOSYS - 10 {
        return;
    }
    // Simple text protocol: "T=<temp> P=<press> V=<0|1>"
    let mut pktbuf: core::mem::MaybeUninit<[u8; 48]> = core::mem::MaybeUninit::uninit();
    let p = pktbuf.as_mut_ptr() as *mut u8;
    let mut pos: usize = 0;

    pos = wstatic(p, pos, S_TEMP.as_ptr(), 2);
    pos = wi32(p, pos, reading.temperature_mc);
    pos = wstatic(p, pos, S_PRESS.as_ptr(), 3);
    pos = wu64(p, pos, reading.pressure_hpa as u64);
    pos = wstatic(p, pos, S_VIB.as_ptr(), 3);
    pos = wu64(p, pos, if reading.vibration { 1 } else { 0 });

    let data = unsafe { core::slice::from_raw_parts(p, pos) };
    sys_net_send(sock_fd, data);
}

// ── Console output ───────────────────────────────────────────────────────────

#[link_section = ".user.text"]
fn print_reading(reading: &SensorReading, sample_num: u64) {
    let mut buf: core::mem::MaybeUninit<[u8; 80]> = core::mem::MaybeUninit::uninit();
    let p = buf.as_mut_ptr() as *mut u8;
    let mut pos: usize = 0;

    pos = wstatic(p, pos, MSG_SAMPLE.as_ptr(), 10);
    pos = wu64(p, pos, sample_num);
    unsafe { core::ptr::write_volatile(p.add(pos), b' '); }
    pos += 1;
    pos = wstatic(p, pos, S_TEMP.as_ptr(), 2);
    pos = wi32(p, pos, reading.temperature_mc);
    pos = wstatic(p, pos, S_MC.as_ptr(), 3);
    pos = wstatic(p, pos, S_PRESS.as_ptr().wrapping_add(1), 2);
    pos = wu64(p, pos, reading.pressure_hpa as u64);
    pos = wstatic(p, pos, S_HPA.as_ptr(), 4);
    pos = wstatic(p, pos, S_VIB.as_ptr(), 3);
    pos = wu64(p, pos, if reading.vibration { 1 } else { 0 });
    pos = wstatic(p, pos, S_NL.as_ptr(), 1);

    sys_write_raw(p, pos);
}

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn sensor_gateway_main(_arg: usize) -> ! {
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // Initialize SPI bus
    sys_write_raw(MSG_INIT_SPI.as_ptr(), MSG_INIT_SPI.len());
    let spi_ok = sys_spi_open(SPI_CLOCK_HZ, 0, 0) == 0;
    if spi_ok {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_STUB.as_ptr(), MSG_STUB.len());
    }

    // Initialize I2C bus
    sys_write_raw(MSG_INIT_I2C.as_ptr(), MSG_INIT_I2C.len());
    let i2c_ok = sys_i2c_open(100_000) == 0;
    if i2c_ok {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_STUB.as_ptr(), MSG_STUB.len());
    }

    // Initialize GPIO pin (input mode = 0)
    sys_write_raw(MSG_INIT_GPIO.as_ptr(), MSG_INIT_GPIO.len());
    let gpio_ok = sys_gpio_set_mode(VIBRATION_GPIO_PIN, 0) == 0;
    if gpio_ok {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_STUB.as_ptr(), MSG_STUB.len());
    }

    // Open UDP socket for telemetry
    sys_write_raw(MSG_INIT_NET.as_ptr(), MSG_INIT_NET.len());
    let sock_fd = sys_net_socket(0); // UDP
    if sock_fd < E_NOSYS - 10 {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
    } else {
        sys_write_raw(MSG_FAIL.as_ptr(), MSG_FAIL.len());
    }

    sys_write_raw(MSG_RUNNING.as_ptr(), MSG_RUNNING.len());

    let mut sample_count: u64 = 0;

    loop {
        sample_count = sample_count.wrapping_add(1);

        let reading = SensorReading {
            timestamp: sys_uptime(),
            temperature_mc: read_temperature_i2c(i2c_ok),
            pressure_hpa: read_pressure_spi(spi_ok),
            vibration: read_vibration_gpio(gpio_ok),
        };

        // Print to console
        print_reading(&reading, sample_count);

        // Log to SD card
        log_reading(&reading);

        // Forward over UDP
        send_reading_udp(sock_fd, &reading);

        sys_delay(SAMPLE_INTERVAL_MS);
    }
}
