# System Dashboard

User-space EL0 application that prints a periodic system status dashboard.

## What It Does

- Reads task ID, uptime, and SoC temperature every 10 seconds
- Tracks the total number of dashboard updates printed
- Formats and prints a bordered dashboard report to the UART console via `SYS_WRITE`
- Runs indefinitely at EL0 as a standard user-space task

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep 10 seconds between dashboard updates |
| `SYS_WRITE` (2) | Print formatted dashboard output to the console |
| `SYS_TASK_ID` (3) | Read the current task's ID for display |
| `SYS_UPTIME` (4) | Read system uptime in milliseconds, displayed as seconds |
| `SYS_TEMPERATURE` (6) | Read SoC temperature in millidegrees Celsius |

## Sample Output

```
[dashboard] started (EL0)
--- tiny-os dashboard ---
  task: 7
  uptime: 10s
  temp: 45.2C
  updates: 1
  status: OK
-------------------------
--- tiny-os dashboard ---
  task: 7
  uptime: 20s
  temp: 45.3C
  updates: 2
  status: OK
-------------------------
```

On QEMU (no VideoCore mailbox), the temperature line shows "no sensor" instead:

```
--- tiny-os dashboard ---
  task: 7
  uptime: 30s
  temp: no sensor
  updates: 3
  status: OK
-------------------------
```

## Notes

- Temperature is returned in millidegrees Celsius (e.g., 45200 = 45.2 C)
- On QEMU, `SYS_TEMPERATURE` returns a negative value; the app displays "no sensor"
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses a fixed-size stack buffer with volatile writes
- The entire dashboard is written in a single `SYS_WRITE` call for clean output
