# Temperature Monitor

User-space EL0 application that monitors the Raspberry Pi 5's SoC temperature.

## What It Does

- Reads the SoC temperature every 5 seconds via the `SYS_TEMPERATURE` syscall
- Tracks min/max/average statistics across all readings
- Prints periodic status updates to the UART console via `SYS_WRITE`
- Runs indefinitely at priority 100 (standard user-space task)

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep between sampling intervals |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_TEMPERATURE` (6) | Read SoC temperature in millidegrees Celsius |

## Sample Output

```
[temp-monitor] started (EL0)
[temp] 45.2C | min 45.0C max 46.1C avg 45.4C | 12 readings
```

## Notes

- Temperature is returned in millidegrees Celsius (e.g., 45200 = 45.2 C)
- On QEMU, `SYS_TEMPERATURE` returns `u64::MAX` (no VideoCore mailbox); the app reports "sensor unavailable"
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
