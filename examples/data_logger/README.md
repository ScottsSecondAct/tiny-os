# Data Logger

User-space EL0 application that creates a file on the FAT32 filesystem and writes timestamped log entries periodically, demonstrating filesystem syscall operations from user space.

## What It Does

- Creates `/LOG.TXT` on the FAT32 filesystem via `SYS_FS` (FS_CREATE)
- Every 15 seconds, reads the system uptime and SoC temperature
- Formats a log entry: `[00001234] temp=45200 status=ok`
- Writes each entry to the file via `SYS_FS` (FS_WRITE)
- Prints a console summary showing entry count, uptime, temperature, and bytes written
- Tracks the total number of entries logged
- Gracefully falls back to console-only logging if file creation fails (e.g., `E_PERM` when the task lacks FS capability)

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_DELAY` (1) | Sleep 15 seconds between log entries |
| `SYS_WRITE` (2) | Print status messages to the UART console |
| `SYS_UPTIME` (4) | Timestamp each log entry (milliseconds) |
| `SYS_TEMPERATURE` (6) | Read SoC temperature in millidegrees Celsius |
| `SYS_FS` (10) | Create log file (op 5), write entries (op 2), close file (op 3) |

## Sample Output

```
[data-logger] started at EL0
[data-logger] creating /LOG.TXT.. denied (no FS capability)
[data-logger] falling back to console only
[data-logger] entry #1 uptime=16234ms temp=45200mC (console only)
[data-logger] entry #2 uptime=31234ms temp=45300mC (console only)
```

With filesystem capability granted:

```
[data-logger] started at EL0
[data-logger] creating /LOG.TXT.. ok
[data-logger] logging to /LOG.TXT ok
[data-logger] entry #1 uptime=16234ms temp=45200mC wrote=31
[data-logger] entry #2 uptime=31234ms temp=45300mC wrote=31
```

Log file contents (`cat /LOG.TXT`):

```
[00016234] temp=45200 status=ok
[00031234] temp=45300 status=ok
```

## Design

- Runs at standard user-space priority (100)
- On QEMU, `SYS_TEMPERATURE` returns `u64::MAX` (no VideoCore mailbox); the raw value is logged
- User tasks lack `CAP_FS` by default, so file creation returns `E_PERM`; the app detects this and continues with console-only output
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
- Uptime in the log entry is zero-padded to 8 digits for consistent alignment

## Educational Value

This example teaches how to:

1. **Use filesystem syscalls from EL0** — the 4-argument `SYS_FS` pattern with operation codes (CREATE, WRITE, CLOSE)
2. **Handle capability denial gracefully** — detect `E_PERM` and degrade to a working fallback
3. **Format data without `alloc` or `std`** — volatile buffer manipulation with `MaybeUninit`
4. **Combine multiple syscall types** — basic syscalls (DELAY, WRITE, UPTIME, TEMPERATURE) alongside subsystem-multiplexed syscalls (SYS_FS)
