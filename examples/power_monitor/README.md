# Power Monitor

User-space EL0 application that monitors CPU frequency, voltage, and temperature.

## What It Does

- Probes `SYS_POWER` capability at startup; falls back to temperature-only mode if denied
- Reads CPU frequency (current/min/max), voltage, and SoC temperature every 8 seconds
- Converts Hz to MHz and microvolts to millivolts for human-readable output
- Tracks min/max temperature across all readings
- Prints a thermal warning when temperature exceeds 70C
- Prints a summary every 10 readings with temperature min/max

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep between sampling intervals |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_TEMPERATURE` (6) | Read SoC temperature in millidegrees Celsius |
| `SYS_POWER` (21) | Read CPU frequency and voltage (requires `CAP_POWER`) |

### SYS_POWER Operations

| Operation | Code | Returns |
|-----------|------|---------|
| `POWER_GET_FREQ` | 0 | Current CPU frequency in Hz |
| `POWER_GET_MAX_FREQ` | 2 | Maximum CPU frequency in Hz |
| `POWER_GET_MIN_FREQ` | 3 | Minimum CPU frequency in Hz |
| `POWER_GET_VOLTAGE` | 4 | CPU core voltage in microvolts |

## Sample Output

With power capability granted:
```
[power-monitor] started (EL0)
[power-monitor] probing power..
[power-monitor] power syscalls: OK
[power] freq=2400MHz min=600MHz max=2400MHz volt=880mV temp=45.2C
[power-monitor] === 10-reading summary ===
[power-monitor]   temp min=44.8C max=46.1C
```

Without power capability (default user task):
```
[power-monitor] started (EL0)
[power-monitor] probing power..
[power-monitor] power denied, temp-only mode
[power] freq=N/A (no capability) temp=45.2C
```

## Notes

- `CAP_POWER` is not included in `CAP_USER_DEFAULT`, so the app gracefully degrades unless the task is explicitly granted the capability
- Temperature is returned in millidegrees Celsius (e.g., 45200 = 45.2C)
- Frequency is returned in Hz and displayed as MHz (divided by 1,000,000)
- Voltage is returned in microvolts and displayed as mV (divided by 1,000)
- On QEMU, `SYS_TEMPERATURE` returns `u64::MAX` (no VideoCore mailbox); the app shows "N/A"
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
