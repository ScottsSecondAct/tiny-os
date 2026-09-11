# LED Blinker

User-space EL0 application that demonstrates capability-aware peripheral programming on tiny_os.

## What It Does

- Attempts to control an LED on GPIO pin 18 and PWM channel 0 via kernel syscalls
- Detects `E_PERM` when GPIO/PWM capabilities are denied (the default for user tasks)
- Falls back to simulation mode, printing what it would do instead of touching hardware
- Cycles through three blink patterns: steady, heartbeat, and SOS (Morse code)
- Waits 10 seconds between pattern switches

## Blink Patterns

| Pattern   | Description                                       |
|-----------|---------------------------------------------------|
| Steady    | Equal 500ms on / 500ms off, 6 cycles              |
| Heartbeat | Two quick 120ms pulses followed by a 600ms pause, 4 cycles |
| SOS       | Morse `... --- ...` (3 short, 3 long, 3 short)    |

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Timing for blink on/off durations and inter-pattern gaps |
| `SYS_WRITE` (2) | Print status and simulation output to the console |
| `SYS_GPIO` (14) | Configure pin 18 as output (`GPIO_SET_MODE`) and toggle it (`GPIO_WRITE`) |
| `SYS_PWM` (16) | Configure channel 0 at 1 kHz (`PWM_CONFIGURE`), set 50% duty (`PWM_SET_DUTY`), enable/disable |

## Capability Handling

User tasks are created with `CAP_USER_DEFAULT`, which excludes `CAP_GPIO` (bit 14) and `CAP_PWM` (bit 16). When the app issues a GPIO or PWM syscall, the kernel returns `E_PERM` (`u64::MAX - 8`). The app checks every return value and enters simulation mode if denied, printing the intended action rather than halting.

To grant GPIO/PWM access, a kernel task would need to set `CAP_GPIO | CAP_PWM` in the task's capability bitmask before creation.

## Sample Output

```
[led-blinker] LED blinker started (EL0)
[led-blinker] probing GPIO cap..    
[led-blinker] GPIO: no capability, sim mode active
[led-blinker] probing PWM cap..     
[led-blinker] PWM: no capability    
[led-blinker] pattern: steady    
[led-blinker] (sim) LED -> ON
[led-blinker] (sim) LED -> OFF
[led-blinker] (sim) LED -> ON
[led-blinker] (sim) LED -> OFF
...
[led-blinker] switching pattern next
[led-blinker] pattern: heartbeat 
[led-blinker] (sim) LED -> ON
[led-blinker] (sim) LED -> OFF
...
[led-blinker] switching pattern next
[led-blinker] pattern: SOS       
[led-blinker] (sim) LED -> ON
[led-blinker] (sim) LED -> OFF
...
```

## Notes

- GPIO and PWM are not in `CAP_USER_DEFAULT`, so this app always runs in simulation mode unless capabilities are explicitly granted
- On QEMU, GPIO/PWM syscalls return stub errors even with capabilities, producing the same simulation behavior
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
- The app never returns (`-> !`); it loops through patterns indefinitely
