//! PLC / Motion Controller — user-space application for tiny_os
//!
//! Runs at EL0. Implements a classic PLC scan cycle at 10ms period with a
//! trapezoidal motion profile. Reads digital inputs via GPIO (emergency stop,
//! start button, home sensor, limit switch), executes ladder-logic-style state
//! machine control rules, and drives servo/stepper outputs via PWM for motor
//! speed control.
//!
//! If GPIO/PWM capabilities are not granted (CAP_USER_DEFAULT excludes them),
//! the app transparently falls back to simulation mode — the state machine and
//! motion profile execute identically, but I/O is emulated so the logic can be
//! verified without hardware.

use core::arch::asm;

// ── Syscall numbers ──────────────────────────────────────────────────────────

const SYS_DELAY: u64 = 1;
const SYS_WRITE: u64 = 2;
const SYS_UPTIME: u64 = 4;
const SYS_GPIO: u64 = 14;
const SYS_PWM: u64 = 16;

// GPIO operations
const GPIO_SET_MODE: u64 = 0;
const GPIO_READ: u64 = 1;
const GPIO_WRITE: u64 = 2;

// PWM operations
const PWM_CONFIGURE: u64 = 0;
const PWM_SET_DUTY: u64 = 1;
const PWM_ENABLE: u64 = 2;
const PWM_DISABLE: u64 = 3;

// Error codes
const E_PERM: u64 = u64::MAX - 8;

// ── GPIO pin assignments ─────────────────────────────────────────────────────

const PIN_ESTOP: u8 = 4; // Emergency stop (active low: 0 = ESTOP active)
const PIN_START: u8 = 17; // Start button
const PIN_HOME: u8 = 27; // Home sensor (axis at home position)
const PIN_LIMIT: u8 = 22; // Limit switch (end of travel)
const PIN_STATUS_LED: u8 = 23; // Status LED output
const PIN_MOTOR_EN: u8 = 24; // Motor enable relay output
const PWM_MOTOR_CH: u8 = 0; // PWM channel for motor speed

// ── Motion profile constants ─────────────────────────────────────────────────

const SCAN_PERIOD_MS: u64 = 10; // 10 ms PLC scan cycle
const DIAG_INTERVAL: u64 = 100; // Print diagnostics every 100 scans (~1s)
const TARGET_SPEED: u64 = 80; // Target duty cycle %
const ACCEL_RATE: u64 = 2; // % per scan cycle
const HOMING_SPEED: u64 = 20; // Slow speed for homing
const TARGET_POSITION: u64 = 5000; // Target position in step counts
const DECEL_DISTANCE: u64 = 800; // Begin deceleration this many steps before target
const PWM_FREQ_HZ: u64 = 20000; // 20 kHz PWM for motor drive

// Simulation timing (in scan cycles)
const SIM_START_PRESS_CYCLE: u64 = 300; // ~3 seconds
const SIM_HOME_FOUND_CYCLE: u64 = 500; // ~5 seconds

// ── PLC states ───────────────────────────────────────────────────────────────

const STATE_IDLE: u8 = 0;
const STATE_HOMING: u8 = 1;
const STATE_RUNNING: u8 = 2;
const STATE_ESTOP: u8 = 3;
const STATE_FAULT: u8 = 4;

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
fn sys_delay(ms: u32) {
    syscall2(SYS_DELAY, ms as u64, 0);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_write_raw(ptr: *const u8, len: usize) {
    syscall2(SYS_WRITE, ptr as u64, len as u64);
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_uptime() -> u64 {
    syscall2(SYS_UPTIME, 0, 0)
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

#[link_section = ".user.text"]
#[inline(always)]
fn sys_gpio_write(pin: u8, val: u8) -> u64 {
    syscall4(SYS_GPIO, GPIO_WRITE, pin as u64, val as u64, 0)
}

// PWM syscalls
#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_configure(channel: u8, freq_hz: u32, duty: u8) -> u64 {
    syscall4(
        SYS_PWM,
        PWM_CONFIGURE,
        channel as u64,
        freq_hz as u64,
        duty as u64,
    )
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_set_duty(channel: u8, duty: u8) -> u64 {
    syscall4(SYS_PWM, PWM_SET_DUTY, channel as u64, duty as u64, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_enable(channel: u8) -> u64 {
    syscall4(SYS_PWM, PWM_ENABLE, channel as u64, 0, 0)
}

#[link_section = ".user.text"]
#[inline(always)]
fn sys_pwm_disable(channel: u8) -> u64 {
    syscall4(SYS_PWM, PWM_DISABLE, channel as u64, 0, 0)
}

// ── String constants (must live in .user.text for EL0 access) ────────────────

#[link_section = ".user.text"]
static MSG_BANNER: [u8; 42] = *b"[plc] PLC motion controller started (EL0)\n";

#[link_section = ".user.text"]
static MSG_SIM_MODE: [u8; 54] = *b"[plc] running in simulation mode (no GPIO/PWM access)\n";

#[link_section = ".user.text"]
static MSG_HW_MODE: [u8; 37] = *b"[plc] GPIO/PWM hardware access ready\n";

#[link_section = ".user.text"]
static MSG_INIT_GPIO: [u8; 25] = *b"[plc] init GPIO pins...  ";

#[link_section = ".user.text"]
static MSG_INIT_PWM: [u8; 25] = *b"[plc] init PWM motor...  ";

#[link_section = ".user.text"]
static MSG_OK: [u8; 3] = *b"ok\n";

#[link_section = ".user.text"]
static MSG_DENIED: [u8; 7] = *b"denied\n";

#[link_section = ".user.text"]
static MSG_SCAN_LOOP: [u8; 29] = *b"[plc] scan loop started 10ms\n";

#[link_section = ".user.text"]
static LBL: [u8; 6] = *b"[plc] ";

#[link_section = ".user.text"]
static S_STATE: [u8; 6] = *b"state=";

#[link_section = ".user.text"]
static S_POS: [u8; 5] = *b" pos=";

#[link_section = ".user.text"]
static S_SPEED: [u8; 7] = *b" speed=";

#[link_section = ".user.text"]
static S_PCT: [u8; 1] = *b"%";

#[link_section = ".user.text"]
static S_CYCLE: [u8; 7] = *b" cycle=";

#[link_section = ".user.text"]
static S_MS: [u8; 2] = *b"ms";

#[link_section = ".user.text"]
static S_NL: [u8; 1] = *b"\n";

// State name strings
#[link_section = ".user.text"]
static ST_IDLE: [u8; 4] = *b"IDLE";

#[link_section = ".user.text"]
static ST_HOMING: [u8; 6] = *b"HOMING";

#[link_section = ".user.text"]
static ST_RUNNING: [u8; 7] = *b"RUNNING";

#[link_section = ".user.text"]
static ST_ESTOP: [u8; 5] = *b"ESTOP";

#[link_section = ".user.text"]
static ST_FAULT: [u8; 5] = *b"FAULT";

// Diagnostic messages
#[link_section = ".user.text"]
static MSG_ESTOP_ACTIVE: [u8; 31] = *b"[plc] ESTOP activated - halted\n";

#[link_section = ".user.text"]
static MSG_ESTOP_CLEAR: [u8; 35] = *b"[plc] ESTOP cleared - returning OK\n";

#[link_section = ".user.text"]
static MSG_HOMING_START: [u8; 24] = *b"[plc] homing axis start\n";

#[link_section = ".user.text"]
static MSG_HOMING_DONE: [u8; 29] = *b"[plc] home found - axis zero\n";

#[link_section = ".user.text"]
static MSG_MOTION_START: [u8; 30] = *b"[plc] motion profile starting\n";

#[link_section = ".user.text"]
static MSG_MOTION_DONE: [u8; 37] = *b"[plc] motion complete - returning OK\n";

#[link_section = ".user.text"]
static MSG_LIMIT_FAULT: [u8; 31] = *b"[plc] FAULT: limit switch hit!\n";

#[link_section = ".user.text"]
static MSG_OVERRUN: [u8; 29] = *b"[plc] WARNING: scan overrun!\n";

// Scan timing diagnostic labels
#[link_section = ".user.text"]
static S_SCAN_MIN: [u8; 5] = *b" min=";

#[link_section = ".user.text"]
static S_SCAN_MAX: [u8; 5] = *b" max=";

#[link_section = ".user.text"]
static S_SCAN_AVG: [u8; 5] = *b" avg=";

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

// ── Helper: check if a return value is a capability denial ──────────────────

#[link_section = ".user.text"]
#[inline(always)]
fn is_error(val: u64) -> bool {
    val >= E_PERM
}

// ── I/O abstraction: hardware or simulation ──────────────────────────────────

/// Inputs read each scan cycle.
#[derive(Clone, Copy)]
struct PlcInputs {
    estop: bool, // true = ESTOP active (pressed / active low)
    start: bool, // true = start button pressed
    home: bool,  // true = home sensor triggered
    limit: bool, // true = limit switch hit
}

/// Read all digital inputs from GPIO, or simulate them.
#[link_section = ".user.text"]
fn read_inputs(sim: bool, cycle: u64) -> PlcInputs {
    if sim {
        // Simulation: manufacture input events at deterministic times
        PlcInputs {
            estop: false, // ESTOP not active in simulation
            start: cycle >= SIM_START_PRESS_CYCLE,
            home: cycle >= SIM_HOME_FOUND_CYCLE,
            limit: false, // No unexpected limit in simulation
        }
    } else {
        // Hardware: read GPIO pins
        let estop_raw = sys_gpio_read(PIN_ESTOP);
        let start_raw = sys_gpio_read(PIN_START);
        let home_raw = sys_gpio_read(PIN_HOME);
        let limit_raw = sys_gpio_read(PIN_LIMIT);
        PlcInputs {
            estop: estop_raw == 0, // Active low
            start: start_raw == 1,
            home: home_raw == 1,
            limit: limit_raw == 1,
        }
    }
}

/// Write status LED (blink pattern varies by state).
#[link_section = ".user.text"]
fn write_status_led(sim: bool, cycle: u64, state: u8) {
    // Blink pattern: IDLE=slow 1Hz, HOMING=fast 5Hz, RUNNING=solid on,
    //                ESTOP=very fast 10Hz, FAULT=off
    let led_on = match state {
        STATE_IDLE => (cycle / 50).is_multiple_of(2), // 1 Hz (50 scans on, 50 off)
        STATE_HOMING => (cycle / 10).is_multiple_of(2), // 5 Hz
        STATE_RUNNING => true,                        // Solid on
        STATE_ESTOP => (cycle / 5).is_multiple_of(2), // 10 Hz
        _ => false,                                   // FAULT = off
    };
    if !sim {
        sys_gpio_write(PIN_STATUS_LED, if led_on { 1 } else { 0 });
    }
}

/// Enable or disable the motor relay.
#[link_section = ".user.text"]
fn write_motor_enable(sim: bool, enable: bool) {
    if !sim {
        sys_gpio_write(PIN_MOTOR_EN, if enable { 1 } else { 0 });
    }
}

/// Set motor speed via PWM duty cycle (0-100%).
#[link_section = ".user.text"]
fn set_motor_speed(sim: bool, duty: u64) {
    if !sim {
        sys_pwm_set_duty(PWM_MOTOR_CH, duty as u8);
    }
}

// ── State name helper ────────────────────────────────────────────────────────

#[link_section = ".user.text"]
fn state_name_ptr_len(state: u8) -> (*const u8, usize) {
    match state {
        STATE_IDLE => (ST_IDLE.as_ptr(), 4),
        STATE_HOMING => (ST_HOMING.as_ptr(), 6),
        STATE_RUNNING => (ST_RUNNING.as_ptr(), 7),
        STATE_ESTOP => (ST_ESTOP.as_ptr(), 5),
        _ => (ST_FAULT.as_ptr(), 5),
    }
}

// ── Diagnostics output ──────────────────────────────────────────────────────

/// Print: "[plc] state=RUNNING pos=1234 speed=80% cycle=1ms min=0 max=1 avg=0\n"
#[link_section = ".user.text"]
fn print_diagnostics(
    state: u8,
    position: u64,
    speed: u64,
    scan_ms: u64,
    scan_min: u64,
    scan_max: u64,
    scan_avg: u64,
) {
    let mut buf: core::mem::MaybeUninit<[u8; 128]> = core::mem::MaybeUninit::uninit();
    let p = buf.as_mut_ptr() as *mut u8;
    let mut pos: usize = 0;

    pos = wstatic(p, pos, LBL.as_ptr(), 6);
    pos = wstatic(p, pos, S_STATE.as_ptr(), 6);
    let (name_ptr, name_len) = state_name_ptr_len(state);
    pos = wstatic(p, pos, name_ptr, name_len);
    pos = wstatic(p, pos, S_POS.as_ptr(), 5);
    pos = wu64(p, pos, position);
    pos = wstatic(p, pos, S_SPEED.as_ptr(), 7);
    pos = wu64(p, pos, speed);
    pos = wstatic(p, pos, S_PCT.as_ptr(), 1);
    pos = wstatic(p, pos, S_CYCLE.as_ptr(), 7);
    pos = wu64(p, pos, scan_ms);
    pos = wstatic(p, pos, S_MS.as_ptr(), 2);
    pos = wstatic(p, pos, S_SCAN_MIN.as_ptr(), 5);
    pos = wu64(p, pos, scan_min);
    pos = wstatic(p, pos, S_SCAN_MAX.as_ptr(), 5);
    pos = wu64(p, pos, scan_max);
    pos = wstatic(p, pos, S_SCAN_AVG.as_ptr(), 5);
    pos = wu64(p, pos, scan_avg);
    pos = wstatic(p, pos, S_NL.as_ptr(), 1);

    sys_write_raw(p, pos);
}

// ── Application entry point ──────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn plc_motion_main(_arg: usize) -> ! {
    sys_write_raw(MSG_BANNER.as_ptr(), MSG_BANNER.len());
    sys_delay(500);

    // ── Probe GPIO/PWM capabilities ──────────────────────────────────────
    // Try to configure one GPIO pin. If E_PERM, fall back to simulation.
    sys_write_raw(MSG_INIT_GPIO.as_ptr(), MSG_INIT_GPIO.len());
    let gpio_probe = sys_gpio_set_mode(PIN_ESTOP, 0); // Input mode
    let sim = is_error(gpio_probe);

    if sim {
        sys_write_raw(MSG_DENIED.as_ptr(), MSG_DENIED.len());
        sys_write_raw(MSG_SIM_MODE.as_ptr(), MSG_SIM_MODE.len());
    } else {
        sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());

        // Configure remaining GPIO input pins
        sys_gpio_set_mode(PIN_START, 0); // Input
        sys_gpio_set_mode(PIN_HOME, 0); // Input
        sys_gpio_set_mode(PIN_LIMIT, 0); // Input

        // Configure GPIO output pins
        sys_gpio_set_mode(PIN_STATUS_LED, 1); // Output
        sys_gpio_set_mode(PIN_MOTOR_EN, 1); // Output

        // Initialize PWM for motor
        sys_write_raw(MSG_INIT_PWM.as_ptr(), MSG_INIT_PWM.len());
        let pwm_probe = sys_pwm_configure(PWM_MOTOR_CH, PWM_FREQ_HZ as u32, 0);
        if is_error(pwm_probe) {
            sys_write_raw(MSG_DENIED.as_ptr(), MSG_DENIED.len());
            // Note: if PWM denied but GPIO ok, we still run — just no motor speed
        } else {
            sys_pwm_enable(PWM_MOTOR_CH);
            sys_write_raw(MSG_OK.as_ptr(), MSG_OK.len());
        }

        sys_write_raw(MSG_HW_MODE.as_ptr(), MSG_HW_MODE.len());
    }

    sys_write_raw(MSG_SCAN_LOOP.as_ptr(), MSG_SCAN_LOOP.len());

    // ── PLC state variables ──────────────────────────────────────────────

    let mut state: u8 = STATE_IDLE;
    let mut position: u64 = 0; // Simulated axis position (step count)
    let mut speed: u64 = 0; // Current speed (duty %)
    let mut cycle_count: u64 = 0; // Total scan cycles

    // Scan timing statistics
    let mut scan_min: u64 = u64::MAX;
    let mut scan_max: u64 = 0;
    let mut scan_sum: u64 = 0;
    let mut scan_timing_count: u64 = 0;

    // Edge detection for start button (rising edge only)
    let mut start_prev: bool = false;

    // ── Main PLC scan loop ───────────────────────────────────────────────

    loop {
        let scan_start = sys_uptime();
        cycle_count = cycle_count.wrapping_add(1);

        // ── Phase 1: Input Scan ──────────────────────────────────────────

        let inputs = read_inputs(sim, cycle_count);

        // Detect start button rising edge
        let start_rising = inputs.start && !start_prev;
        start_prev = inputs.start;

        // ── Phase 2: Logic Execution ─────────────────────────────────────

        // Global ESTOP check — overrides all states
        if inputs.estop {
            if state != STATE_ESTOP {
                state = STATE_ESTOP;
                speed = 0;
                set_motor_speed(sim, 0);
                write_motor_enable(sim, false);
                if !sim {
                    sys_pwm_disable(PWM_MOTOR_CH);
                }
                sys_write_raw(MSG_ESTOP_ACTIVE.as_ptr(), MSG_ESTOP_ACTIVE.len());
            }
        } else {
            match state {
                // ── IDLE: wait for start button rising edge ──────────
                STATE_IDLE => {
                    speed = 0;
                    if start_rising {
                        state = STATE_HOMING;
                        sys_write_raw(MSG_HOMING_START.as_ptr(), MSG_HOMING_START.len());
                        write_motor_enable(sim, true);
                        if !sim {
                            sys_pwm_enable(PWM_MOTOR_CH);
                        }
                    }
                }

                // ── HOMING: move toward home sensor at slow speed ────
                STATE_HOMING => {
                    // Limit switch during homing is a fault
                    if inputs.limit {
                        state = STATE_FAULT;
                        speed = 0;
                        set_motor_speed(sim, 0);
                        write_motor_enable(sim, false);
                        sys_write_raw(MSG_LIMIT_FAULT.as_ptr(), MSG_LIMIT_FAULT.len());
                    } else if inputs.home {
                        // Home found — zero position and begin motion
                        position = 0;
                        speed = 0;
                        state = STATE_RUNNING;
                        sys_write_raw(MSG_HOMING_DONE.as_ptr(), MSG_HOMING_DONE.len());
                        sys_write_raw(MSG_MOTION_START.as_ptr(), MSG_MOTION_START.len());
                    } else {
                        // Ramp up to homing speed
                        if speed < HOMING_SPEED {
                            speed = speed.wrapping_add(ACCEL_RATE);
                            if speed > HOMING_SPEED {
                                speed = HOMING_SPEED;
                            }
                        }
                        set_motor_speed(sim, speed);
                    }
                }

                // ── RUNNING: trapezoidal motion profile ──────────────
                STATE_RUNNING => {
                    if inputs.limit {
                        // Unexpected limit switch — fault
                        state = STATE_FAULT;
                        speed = 0;
                        set_motor_speed(sim, 0);
                        write_motor_enable(sim, false);
                        sys_write_raw(MSG_LIMIT_FAULT.as_ptr(), MSG_LIMIT_FAULT.len());
                    } else {
                        let remaining = TARGET_POSITION.saturating_sub(position);

                        if remaining == 0 {
                            // Motion complete — stop and return to idle
                            speed = 0;
                            set_motor_speed(sim, 0);
                            write_motor_enable(sim, false);
                            if !sim {
                                sys_pwm_disable(PWM_MOTOR_CH);
                            }
                            state = STATE_IDLE;
                            sys_write_raw(MSG_MOTION_DONE.as_ptr(), MSG_MOTION_DONE.len());
                        } else if remaining <= DECEL_DISTANCE {
                            // Deceleration zone: ramp down proportionally
                            let decel_target = (remaining * TARGET_SPEED) / DECEL_DISTANCE;
                            let min_speed: u64 = 5; // minimum creep speed
                            let target = if decel_target > min_speed {
                                decel_target
                            } else {
                                min_speed
                            };
                            if speed > target {
                                if speed >= ACCEL_RATE {
                                    speed = speed.wrapping_sub(ACCEL_RATE);
                                } else {
                                    speed = 0;
                                }
                                if speed < target {
                                    speed = target;
                                }
                            }
                            set_motor_speed(sim, speed);
                            // Advance position proportional to speed
                            position = position.wrapping_add(speed);
                        } else {
                            // Acceleration / constant speed zone
                            if speed < TARGET_SPEED {
                                speed = speed.wrapping_add(ACCEL_RATE);
                                if speed > TARGET_SPEED {
                                    speed = TARGET_SPEED;
                                }
                            }
                            set_motor_speed(sim, speed);
                            // Advance position proportional to speed
                            position = position.wrapping_add(speed);
                        }
                    }
                }

                // ── ESTOP: wait for ESTOP to clear ───────────────────
                STATE_ESTOP => {
                    // ESTOP cleared (handled by outer if — reaching here means
                    // estop is no longer active)
                    state = STATE_IDLE;
                    position = 0;
                    sys_write_raw(MSG_ESTOP_CLEAR.as_ptr(), MSG_ESTOP_CLEAR.len());
                }

                // ── FAULT: wait for operator reset (start button) ────
                _ => {
                    // STATE_FAULT
                    speed = 0;
                    if start_rising {
                        // Operator acknowledged fault — return to idle
                        state = STATE_IDLE;
                        position = 0;
                    }
                }
            }
        }

        // ── Phase 3: Output Update ───────────────────────────────────────

        write_status_led(sim, cycle_count, state);

        // ── Phase 4: Diagnostics ─────────────────────────────────────────

        let scan_end = sys_uptime();
        let scan_elapsed = scan_end.wrapping_sub(scan_start);

        // Track scan timing statistics
        if scan_elapsed < scan_min {
            scan_min = scan_elapsed;
        }
        if scan_elapsed > scan_max {
            scan_max = scan_elapsed;
        }
        scan_sum = scan_sum.wrapping_add(scan_elapsed);
        scan_timing_count = scan_timing_count.wrapping_add(1);

        // Log state transition messages are already printed above.

        // Periodic diagnostics every DIAG_INTERVAL scans
        if cycle_count.is_multiple_of(DIAG_INTERVAL) {
            let scan_avg = scan_sum.checked_div(scan_timing_count).unwrap_or(0);
            print_diagnostics(
                state,
                position,
                speed,
                scan_elapsed,
                scan_min,
                scan_max,
                scan_avg,
            );
        }

        // Warn on overrun
        if scan_elapsed > SCAN_PERIOD_MS {
            sys_write_raw(MSG_OVERRUN.as_ptr(), MSG_OVERRUN.len());
        }

        // ── Scan cycle timing ────────────────────────────────────────────
        // Delay for the remainder of the 10ms scan period
        if scan_elapsed < SCAN_PERIOD_MS {
            let wait = (SCAN_PERIOD_MS - scan_elapsed) as u32;
            sys_delay(wait);
        }
        // If overrun, continue immediately (no skip)
    }
}
