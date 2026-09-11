# PLC / Motion Controller

User-space EL0 application that implements a PLC-style scan cycle with trapezoidal motion control. Reads digital inputs via GPIO, executes ladder-logic-style state machine rules, and drives servo/stepper outputs via PWM.

## What It Demonstrates

- **PLC scan cycle** at a fixed 10ms period with input scan, logic execution, and output update phases -- the classic IEC 61131-3 pattern used in industrial programmable logic controllers
- **Ladder-logic state machine** with five states (IDLE, HOMING, RUNNING, ESTOP, FAULT) implementing safety interlocks, homing sequences, and fault handling
- **Trapezoidal motion profile** with acceleration ramp, constant velocity, and deceleration zone -- the fundamental motion primitive for servo drives and stepper controllers
- **Capability-aware simulation** that detects E_PERM at startup and falls back to simulated I/O, letting the PLC logic run on QEMU without hardware

## State Machine

```
                   START btn
      +-------+  (rising edge)  +---------+
      | IDLE  |---------------->| HOMING  |
      +-------+                 +---------+
        ^   ^                     |     |
        |   |            home     |     | limit
        |   |           sensor    |     | switch
        |   |                     v     v
        |   |   motion done   +---------+     +-------+
        |   +<----------------| RUNNING |---->| FAULT |
        |                     +---------+     +-------+
        |                         |               |
        |       ESTOP (any state) |    START btn  |
        |            |            v   (ack reset) |
        |            +------> +-------+ <---------+
        |   ESTOP cleared     | ESTOP |
        +<--------------------+-------+
```

**State descriptions:**

| State | Behavior | LED Pattern |
|-------|----------|-------------|
| IDLE | Waiting for start button rising edge | Slow blink (1 Hz) |
| HOMING | Moving toward home sensor at 20% speed | Fast blink (5 Hz) |
| RUNNING | Executing trapezoidal motion profile | Solid on |
| ESTOP | All outputs off; waiting for ESTOP release | Very fast blink (10 Hz) |
| FAULT | Limit switch hit unexpectedly; waiting for operator reset | Off |

## Motion Profile

```
Speed %
  80 |          ___________________
     |         /                   \
     |        /                     \
     |       /                       \
     |      /    accel: +2%/scan      \ decel: -2%/scan
   0 |_____/                           \_____
     |-----|----------|---------|------|
     0   ramp up   constant   decel  target
                   velocity          position
```

- Target speed: 80% duty cycle
- Acceleration rate: 2% per 10ms scan cycle (full ramp in 400ms)
- Deceleration begins 800 steps before target position (5000 steps)
- Minimum creep speed during deceleration: 5%

## GPIO Pin Map

| Pin | Direction | Function |
|-----|-----------|----------|
| 4 | Input | Emergency stop (active low -- 0 = ESTOP active) |
| 17 | Input | Start button |
| 27 | Input | Home sensor (axis at home position) |
| 22 | Input | Limit switch (end of travel) |
| 23 | Output | Status LED (blink pattern varies by state) |
| 24 | Output | Motor enable relay |
| PWM Ch0 | Output | Motor speed (0-100% duty, 20 kHz) |

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Scan cycle timing -- sleep remainder of 10ms period |
| `SYS_WRITE` (2) | Print diagnostics and state transitions to UART |
| `SYS_UPTIME` (4) | Measure scan cycle elapsed time |
| `SYS_GPIO` (14) | Read digital inputs, write LED and relay outputs |
| `SYS_PWM` (16) | Configure and set motor speed duty cycle |

## Capability-Aware Simulation

GPIO and PWM are **not** included in `CAP_USER_DEFAULT` -- user tasks must be explicitly granted `CAP_GPIO` and `CAP_PWM` capabilities. The app probes for access at startup by attempting `GPIO_SET_MODE` on pin 4:

- **If GPIO returns `E_PERM`:** the app enters simulation mode. All I/O is emulated:
  - Simulated start button press after ~3 seconds (300 scan cycles)
  - Simulated home sensor trigger after ~5 seconds (500 scan cycles)
  - No ESTOP or limit-switch events in simulation
  - PWM/GPIO writes are skipped; the state machine runs identically
- **If GPIO succeeds:** real hardware I/O is used for all pins and PWM

This design means the PLC logic can be fully verified on QEMU without any hardware, and the same binary runs on real Pi 5 hardware with GPIO/PWM capabilities granted.

## Scan Cycle Timing

Each scan measures elapsed time via `SYS_UPTIME` (1ms tick resolution):
- If the scan takes less than 10ms, the app delays the remainder
- If the scan overruns 10ms, a warning is printed but execution continues (no skip)
- Min/max/average scan times are tracked and reported every 100 cycles (~1 second)

## Sample Output (Simulation Mode)

```
[plc] PLC motion controller started (EL0)
[plc] init GPIO pins..  denied
[plc] running in simulation mode (no GPIO/PWM access)
[plc] scan loop started 10ms
[plc] state=IDLE pos=0 speed=0% cycle=0ms min=0 max=0 avg=0
[plc] homing axis start
[plc] state=HOMING pos=0 speed=20% cycle=0ms min=0 max=0 avg=0
[plc] home found - axis zero
[plc] motion profile starting
[plc] state=RUNNING pos=1234 speed=80% cycle=0ms min=0 max=1 avg=0
[plc] motion complete - returning OK
[plc] state=IDLE pos=0 speed=0% cycle=0ms min=0 max=1 avg=0
```

## Real-World Relevance

This example maps directly to real industrial automation patterns:

- **IEC 61131-3 scan cycle:** The input-logic-output structure mirrors how real PLCs (Siemens S7, Allen-Bradley, Beckhoff) execute their programs
- **Servo/stepper drives:** The trapezoidal profile is the baseline for EtherCAT servo drives (CSP/CSV modes), stepper controllers (AccelStepper), and CNC G-code interpreters
- **Safety interlocks:** The ESTOP circuit (active-low, overrides all states) follows IEC 60204-1 Category 0 stop requirements; the limit-switch fault detection prevents mechanical damage
- **Homing sequence:** Standard practice for CNC machines, robotic arms, and linear actuators -- the axis must find a known reference before commanded motion
- **Deterministic timing:** The fixed 10ms scan with overrun detection matches real PLC cycle monitoring (e.g., Siemens OB1 cycle time watchdog)
