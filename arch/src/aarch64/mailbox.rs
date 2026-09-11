use core::ptr::{read_volatile, write_volatile};

const MAILBOX_READ: usize = 0x00;
const MAILBOX_STATUS: usize = 0x18;
const MAILBOX_WRITE: usize = 0x20;

const STATUS_FULL: u32 = 1 << 31;
const STATUS_EMPTY: u32 = 1 << 30;

const CHANNEL_PROPERTY: u32 = 8;

const TAG_GET_TEMPERATURE: u32 = 0x0003_0006;
const TAG_END: u32 = 0;

const RESPONSE_SUCCESS: u32 = 0x8000_0000;

static mut BASE: usize = 0;

#[repr(C, align(16))]
struct PropBuf {
    data: [u32; 8],
}

static mut PROP_BUF: PropBuf = PropBuf { data: [0; 8] };

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
