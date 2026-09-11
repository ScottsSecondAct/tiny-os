# RTC Clock

User-space EL0 application that reads the real-time clock, sets periodic alarms, and demonstrates time-based scheduling via SYS_RTC syscalls.

## What It Does

- Probes for RTC capability at startup; falls back to uptime-only mode if `CAP_RTC` is not granted
- Reads the current time via `RTC_GET_TIME` (DateTime struct via pointer)
- Converts to epoch seconds and displays as `T+DDDd HH:MM:SS` (days since epoch, zero-padded hours/minutes/seconds)
- Sets an alarm for 60 seconds in the future via `RTC_SET_ALARM`
- Checks each loop iteration whether the alarm has fired (epoch comparison)
- When an alarm fires, prints a notification and sets the next alarm (+60s)
- Tracks total number of alarms fired
- Reports every 5 seconds

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep between reporting intervals |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_UPTIME` (4) | Read system tick counter (1kHz), converted to seconds |
| `SYS_RTC` (17) | Read time (`RTC_GET_TIME=0`), set alarm (`RTC_SET_ALARM=2`) |

## RTC Syscall Interface

The kernel's RTC syscalls use `DateTime` struct pointers (not raw epoch values):

- **RTC_GET_TIME** (op=0): `X1` = pointer to `DateTime` struct (kernel writes current time)
- **RTC_SET_ALARM** (op=2): `X1` = pointer to `DateTime` struct (kernel reads alarm time)

The `DateTime` struct matches `arch::rtc::DateTime`: `{ year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8 }`.

The application converts between `DateTime` and epoch seconds internally for arithmetic and display.

## Sample Output

With RTC capability:
```
[rtc-clock] started (EL0)
[rtc-clock] RTC available
[rtc-clock] alarm set (+60s)
[rtc] T+0d 00:00:12 (epoch 12) uptime=13s
[rtc] T+0d 00:00:17 (epoch 17) uptime=18s
[rtc] T+0d 00:01:12 (epoch 72) uptime=73s
[rtc] ALARM fired! (#1)
```

Without RTC capability (default for user tasks):
```
[rtc-clock] started (EL0)
[rtc-clock] no CAP_RTC -- uptime-only mode
[rtc] uptime=6s (no RTC capability)
[rtc] uptime=11s (no RTC capability)
```

## Notes

- `CAP_RTC` is **not** included in `CAP_USER_DEFAULT`, so user tasks will see `E_PERM` unless granted the capability explicitly
- The software RTC starts at epoch 0 (1970-01-01 00:00:00) and counts upward from boot time unless `RTC_SET_TIME` is called
- On QEMU, the RTC is purely software-based (tick-driven); on real Pi 5 hardware, the same software RTC applies
- The display uses simple epoch arithmetic (days = epoch/86400, etc.) rather than full calendar formatting
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
