//! LED Blinker — user-space application for tiny_os
//!
//! Runs at EL0. Demonstrates capability-aware peripheral programming by
//! attempting to control an LED via GPIO and PWM syscalls. When the required
//! capabilities (CAP_GPIO, CAP_PWM) are not granted — which is the default
//! for user tasks — the app detects E_PERM and falls back to simulation
//! mode, printing what it *would* do instead of touching hardware.
//!
//! Cycles through three blink patterns: steady, heartbeat, and SOS.
//! Educational value: shows how EL0 apps gracefully degrade when peripheral
//! capabilities are denied by the kernel's capability bitmask.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_GPIO: u64 = 14;
const SYS_PWM: u64 = 16;

// GPIO operations
const GPIO_SET_MODE: u64 = 0;
const GPIO_WRITE: u64 = 2;

// PWM operations
const PWM_CONFIGURE: u64 = 0;
const PWM_SET_DUTY: u64 = 1;
const PWM_ENABLE: u64 = 2;
const PWM_DISABLE: u64 = 3;

// Error codes
const E_PERM: u64 = u64::MAX - 8;
const E_NOSYS: u64 = u64::MAX;

// Hardware config
const LED_PIN: u64 = 18;
const GPIO_MODE_OUTPUT: u64 = 1;
const PWM_CHANNEL: u64 = 0;
const PWM_FREQ_HZ: u64 = 1000;
const PWM_DUTY_PCT: u64 = 50;

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
fn sys_delay(ms: u32) { syscall(SYS_DELAY, ms as u64, 0); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) { syscall(SYS_WRITE, ptr as u64, len as u64); }

#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_set_mode(pin: u64, mode: u64) -> u64 {
    syscall4(SYS_GPIO, GPIO_SET_MODE, pin, mode, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_write(pin: u64, value: u64) -> u64 {
    syscall4(SYS_GPIO, GPIO_WRITE, pin, value, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_configure(channel: u64, freq_hz: u64) -> u64 {
    syscall4(SYS_PWM, PWM_CONFIGURE, channel, freq_hz, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_set_duty(channel: u64, duty_pct: u64) -> u64 {
    syscall4(SYS_PWM, PWM_SET_DUTY, channel, duty_pct, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_enable(channel: u64) -> u64 {
    syscall4(SYS_PWM, PWM_ENABLE, channel, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_disable(channel: u64) -> u64 {
    syscall4(SYS_PWM, PWM_DISABLE, channel, 0, 0)
}

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 40] = *b"[led-blinker] LED blinker started (EL0)\n";

#[link_section = ".user.text"]
static MSG_GPIO_PROBE: [u8; 36] = *b"[led-blinker] probing GPIO cap..    ";

#[link_section = ".user.text"]
static MSG_GPIO_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_GPIO_DENIED: [u8; 51] = *b"[led-blinker] GPIO: no capability, sim mode active\n";

#[link_section = ".user.text"]
static MSG_PWM_PROBE: [u8; 36] = *b"[led-blinker] probing PWM cap..     ";

#[link_section = ".user.text"]
static MSG_PWM_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_PWM_DENIED: [u8; 37] = *b"[led-blinker] PWM: no capability    \n";

#[link_section = ".user.text"]
static MSG_PWM_CFG_OK: [u8; 39] = *b"[led-blinker] PWM ch0 1kHz 50% enabled\n";

#[link_section = ".user.text"]
static MSG_PATTERN_STEADY: [u8; 34] = *b"[led-blinker] pattern: steady    \n";

#[link_section = ".user.text"]
static MSG_PATTERN_HEARTBEAT: [u8; 34] = *b"[led-blinker] pattern: heartbeat \n";

#[link_section = ".user.text"]
static MSG_PATTERN_SOS: [u8; 34] = *b"[led-blinker] pattern: SOS       \n";

#[link_section = ".user.text"]
static MSG_SIM_ON: [u8; 30] = *b"[led-blinker] (sim) LED -> ON\n";

#[link_section = ".user.text"]
static MSG_SIM_OFF: [u8; 31] = *b"[led-blinker] (sim) LED -> OFF\n";

#[link_section = ".user.text"]
static MSG_HW_ON: [u8; 26] = *b"[led-blinker] LED -> ON  \n";

#[link_section = ".user.text"]
static MSG_HW_OFF: [u8; 26] = *b"[led-blinker] LED -> OFF \n";

#[link_section = ".user.text"]
static MSG_NEXT: [u8; 37] = *b"[led-blinker] switching pattern next\n";

// ── LED control helpers ──────────────────────────────────────────────────────

#[link_section = ".user.text"]
fn led_set(on: bool, gpio_ok: bool) {
    if gpio_ok {
        sys_gpio_write(LED_PIN, if on { 1 } else { 0 });
        if on {
            sys_write_raw(MSG_HW_ON.as_ptr(), MSG_HW_ON.len());
        } else {
            sys_write_raw(MSG_HW_OFF.as_ptr(), MSG_HW_OFF.len());
        }
    } else {
        if on {
            sys_write_raw(MSG_SIM_ON.as_ptr(), MSG_SIM_ON.len());
        } else {
            sys_write_raw(MSG_SIM_OFF.as_ptr(), MSG_SIM_OFF.len());
        }
    }
}

/// Blink: turn LED on for `on_ms`, off for `off_ms`.
#[link_section = ".user.text"]
fn blink(on_ms: u32, off_ms: u32, gpio_ok: bool) {
    led_set(true, gpio_ok);
    sys_delay(on_ms);
    led_set(false, gpio_ok);
    sys_delay(off_ms);
}

// ── Blink patterns ──────────────────────────────────────────────────────────

/// Pattern 1 — Steady blink: equal on/off (500ms each), 6 cycles.
#[link_section = ".user.text"]
fn pattern_steady(gpio_ok: bool) {
    sys_write_raw(MSG_PATTERN_STEADY.as_ptr(), MSG_PATTERN_STEADY.len());
    let mut i: u32 = 0;
    while i < 6 {
        blink(500, 500, gpio_ok);
        i += 1;
    }
}

/// Pattern 2 — Heartbeat: two quick blinks then a long pause, 4 cycles.
#[link_section = ".user.text"]
fn pattern_heartbeat(gpio_ok: bool) {
    sys_write_raw(MSG_PATTERN_HEARTBEAT.as_ptr(), MSG_PATTERN_HEARTBEAT.len());
    let mut i: u32 = 0;
    while i < 4 {
        // First beat
        blink(120, 120, gpio_ok);
        // Second beat
        blink(120, 600, gpio_ok);
        i += 1;
    }
}

/// Pattern 3 — SOS (... --- ...): 3 short, 3 long, 3 short, then word gap.
#[link_section = ".user.text"]
fn pattern_sos(gpio_ok: bool) {
    sys_write_raw(MSG_PATTERN_SOS.as_ptr(), MSG_PATTERN_SOS.len());

    // S: three short blinks (dit)
    let mut i: u32 = 0;
    while i < 3 {
        blink(150, 150, gpio_ok);
        i += 1;
    }
    // Inter-character gap
    sys_delay(300);

    // O: three long blinks (dah)
    i = 0;
    while i < 3 {
        blink(450, 150, gpio_ok);
        i += 1;
    }
    // Inter-character gap
    sys_delay(300);

    // S: three short blinks (dit)
    i = 0;
    while i < 3 {
        blink(150, 150, gpio_ok);
        i += 1;
    }
    // Word gap
    sys_delay(700);
}

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn led_blinker_main(_arg: usize) -> ! {
    // ── Startup banner ──────────────────────────────────────────────────
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // ── Probe GPIO capability ───────────────────────────────────────────
    sys_write_raw(MSG_GPIO_PROBE.as_ptr(), MSG_GPIO_PROBE.len());
    let gpio_ret = sys_gpio_set_mode(LED_PIN, GPIO_MODE_OUTPUT);
    let gpio_ok = gpio_ret == 0;

    if gpio_ok {
        sys_write_raw(MSG_GPIO_OK.as_ptr(), MSG_GPIO_OK.len());
    } else {
        // E_PERM or E_NOSYS — either way, no GPIO access
        sys_write_raw(MSG_GPIO_DENIED.as_ptr(), MSG_GPIO_DENIED.len());
    }

    // ── Probe PWM capability ────────────────────────────────────────────
    sys_write_raw(MSG_PWM_PROBE.as_ptr(), MSG_PWM_PROBE.len());
    let pwm_cfg = sys_pwm_configure(PWM_CHANNEL, PWM_FREQ_HZ);
    let pwm_ok = pwm_cfg == 0;

    if pwm_ok {
        sys_write_raw(MSG_PWM_OK.as_ptr(), MSG_PWM_OK.len());
        // Set duty cycle and enable
        sys_pwm_set_duty(PWM_CHANNEL, PWM_DUTY_PCT);
        sys_pwm_enable(PWM_CHANNEL);
        sys_write_raw(MSG_PWM_CFG_OK.as_ptr(), MSG_PWM_CFG_OK.len());
    } else {
        sys_write_raw(MSG_PWM_DENIED.as_ptr(), MSG_PWM_DENIED.len());
    }

    // ── Main loop: cycle through patterns ───────────────────────────────
    loop {
        // Pattern 1: steady blink
        pattern_steady(gpio_ok);

        sys_write_raw(MSG_NEXT.as_ptr(), MSG_NEXT.len());
        sys_delay(10_000);

        // Pattern 2: heartbeat
        pattern_heartbeat(gpio_ok);

        sys_write_raw(MSG_NEXT.as_ptr(), MSG_NEXT.len());
        sys_delay(10_000);

        // Pattern 3: SOS
        pattern_sos(gpio_ok);

        sys_write_raw(MSG_NEXT.as_ptr(), MSG_NEXT.len());
        sys_delay(10_000);
    }
}
