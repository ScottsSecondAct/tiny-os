use core::ptr::{read_volatile, write_volatile};

const MAILBOX_READ: usize = 0x00;
const MAILBOX_STATUS: usize = 0x18;
const MAILBOX_WRITE: usize = 0x20;

const STATUS_FULL: u32 = 1 << 31;
const STATUS_EMPTY: u32 = 1 << 30;

const CHANNEL_PROPERTY: u32 = 8;

const TAG_GET_TEMPERATURE: u32 = 0x0003_0006;
const TAG_GET_CLOCK_RATE: u32 = 0x0003_0002;
const TAG_SET_CLOCK_RATE: u32 = 0x0003_8002;
const TAG_GET_MAX_CLOCK: u32 = 0x0003_0004;
const TAG_GET_MIN_CLOCK: u32 = 0x0003_0007;
const TAG_GET_VOLTAGE: u32 = 0x0003_0003;
const CLOCK_ARM: u32 = 3;
const TAG_END: u32 = 0;

const RESPONSE_SUCCESS: u32 = 0x8000_0000;

static mut BASE: usize = 0;

#[repr(C, align(16))]
struct PropBuf {
    data: [u32; 8],
}

static mut PROP_BUF: PropBuf = PropBuf { data: [0; 8] };

#[repr(C, align(16))]
struct PropBuf10 {
    data: [u32; 10],
}

static mut PROP_BUF2: PropBuf10 = PropBuf10 { data: [0; 10] };

pub fn init(base: usize) {
    // SAFETY: Called once from kmain before any mailbox use.
    unsafe { BASE = base; }
}

pub fn is_initialized() -> bool {
    unsafe { BASE != 0 }
}

/// Read SoC temperature via VideoCore mailbox property tag.
/// Returns temperature in millidegrees Celsius, or None on failure.
/// QEMU raspi4b returns 25000 (25.0°C).
pub fn get_temperature() -> Option<i32> {
    if !is_initialized() {
        return None;
    }

    // SAFETY: PROP_BUF is only accessed by mailbox calls which are serialized
    // (single sensor task, shell commands block on delay). The buffer address
    // is in the kernel image (<4GB), valid for the 32-bit mailbox register.
    unsafe {
        PROP_BUF.data[0] = 32;
        PROP_BUF.data[1] = 0; // request
        PROP_BUF.data[2] = TAG_GET_TEMPERATURE;
        PROP_BUF.data[3] = 8; // value buffer size
        PROP_BUF.data[4] = 0; // request indicator
        PROP_BUF.data[5] = 0; // temperature ID (0 = SoC)
        PROP_BUF.data[6] = 0; // value (filled by firmware)
        PROP_BUF.data[7] = TAG_END;

        core::arch::asm!("dsb sy");

        let buf_addr = &raw const PROP_BUF as usize as u32;
        if !mailbox_call(buf_addr) {
            return None;
        }

        core::arch::asm!("dsb sy");

        if PROP_BUF.data[1] & RESPONSE_SUCCESS == 0 {
            return None;
        }
        // Bit 31 of data[4] set means response; low bits = response length
        if PROP_BUF.data[4] & RESPONSE_SUCCESS == 0 {
            return None;
        }

        Some(PROP_BUF.data[6] as i32)
    }
}

/// Get the current ARM CPU clock rate in Hz.
pub fn get_clock_rate() -> Option<u32> {
    get_clock_property(TAG_GET_CLOCK_RATE)
}

/// Get the maximum ARM CPU clock rate in Hz.
pub fn get_max_clock() -> Option<u32> {
    get_clock_property(TAG_GET_MAX_CLOCK)
}

/// Get the minimum ARM CPU clock rate in Hz.
pub fn get_min_clock() -> Option<u32> {
    get_clock_property(TAG_GET_MIN_CLOCK)
}

/// Get core voltage in microvolts.
pub fn get_voltage() -> Option<u32> {
    if !is_initialized() {
        return None;
    }

    // SAFETY: Same serialization guarantees as get_temperature.
    unsafe {
        PROP_BUF.data[0] = 32;
        PROP_BUF.data[1] = 0;
        PROP_BUF.data[2] = TAG_GET_VOLTAGE;
        PROP_BUF.data[3] = 8;
        PROP_BUF.data[4] = 0;
        PROP_BUF.data[5] = 1; // voltage_id = 1 (core)
        PROP_BUF.data[6] = 0;
        PROP_BUF.data[7] = TAG_END;

        core::arch::asm!("dsb sy");

        let buf_addr = &raw const PROP_BUF as usize as u32;
        if !mailbox_call(buf_addr) {
            return None;
        }

        core::arch::asm!("dsb sy");

        if PROP_BUF.data[1] & RESPONSE_SUCCESS == 0 {
            return None;
        }
        if PROP_BUF.data[4] & RESPONSE_SUCCESS == 0 {
            return None;
        }

        Some(PROP_BUF.data[6])
    }
}

/// Set the ARM CPU clock rate. Returns the actual rate set, in Hz.
pub fn set_clock_rate(hz: u32) -> Option<u32> {
    if !is_initialized() {
        return None;
    }

    // SAFETY: PROP_BUF2 is only accessed by serialized mailbox calls.
    unsafe {
        PROP_BUF2.data[0] = 40; // total buffer size
        PROP_BUF2.data[1] = 0;  // request
        PROP_BUF2.data[2] = TAG_SET_CLOCK_RATE;
        PROP_BUF2.data[3] = 12; // value buffer size (clock_id + rate + skip_turbo)
        PROP_BUF2.data[4] = 0;  // request indicator
        PROP_BUF2.data[5] = CLOCK_ARM;
        PROP_BUF2.data[6] = hz;
        PROP_BUF2.data[7] = 0; // skip_turbo = 0
        PROP_BUF2.data[8] = TAG_END;
        PROP_BUF2.data[9] = 0;

        core::arch::asm!("dsb sy");

        let buf_addr = &raw const PROP_BUF2 as usize as u32;
        if !mailbox_call(buf_addr) {
            return None;
        }

        core::arch::asm!("dsb sy");

        if PROP_BUF2.data[1] & RESPONSE_SUCCESS == 0 {
            return None;
        }
        if PROP_BUF2.data[4] & RESPONSE_SUCCESS == 0 {
            return None;
        }

        Some(PROP_BUF2.data[6])
    }
}

fn get_clock_property(tag: u32) -> Option<u32> {
    if !is_initialized() {
        return None;
    }

    // SAFETY: Same serialization guarantees as get_temperature.
    unsafe {
        PROP_BUF.data[0] = 32;
        PROP_BUF.data[1] = 0;
        PROP_BUF.data[2] = tag;
        PROP_BUF.data[3] = 8;
        PROP_BUF.data[4] = 0;
        PROP_BUF.data[5] = CLOCK_ARM;
        PROP_BUF.data[6] = 0;
        PROP_BUF.data[7] = TAG_END;

        core::arch::asm!("dsb sy");

        let buf_addr = &raw const PROP_BUF as usize as u32;
        if !mailbox_call(buf_addr) {
            return None;
        }

        core::arch::asm!("dsb sy");

        if PROP_BUF.data[1] & RESPONSE_SUCCESS == 0 {
            return None;
        }
        if PROP_BUF.data[4] & RESPONSE_SUCCESS == 0 {
            return None;
        }

        Some(PROP_BUF.data[6])
    }
}

fn mailbox_call(buf_addr: u32) -> bool {
    let base = unsafe { BASE };
    let val = (buf_addr & !0xF) | CHANNEL_PROPERTY;

    // SAFETY: BASE points to valid MMIO mailbox registers.
    unsafe {
        // Wait for mailbox not full.
        let mut timeout = 1_000_000u32;
        while read_volatile((base + MAILBOX_STATUS) as *const u32) & STATUS_FULL != 0 {
            timeout -= 1;
            if timeout == 0 {
                return false;
            }
            core::hint::spin_loop();
        }

        write_volatile((base + MAILBOX_WRITE) as *mut u32, val);

        // Wait for response on our channel.
        timeout = 1_000_000;
        loop {
            while read_volatile((base + MAILBOX_STATUS) as *const u32) & STATUS_EMPTY != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return false;
                }
                core::hint::spin_loop();
            }
            let response = read_volatile((base + MAILBOX_READ) as *const u32);
            if response & 0xF == CHANNEL_PROPERTY {
                return true;
            }
            timeout -= 1;
            if timeout == 0 {
                return false;
            }
        }
    }
}
