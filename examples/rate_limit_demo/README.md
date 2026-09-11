# Rate Limit Demo

User-space EL0 application that demonstrates Phase 14's per-task syscall rate limiting.

## What It Does

- Shows normal syscall operation (10 calls spread over 5 seconds, all succeed)
- Bursts 600 rapid `sys_yield()` calls in a tight loop to trigger the rate limiter
- Counts how many succeed vs return `E_RATE_LIMIT` (`u64::MAX - 9`)
- Waits 2 seconds for the sliding window to reset, then verifies recovery
- Repeats the cycle every 30 seconds

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_YIELD` (0) | Primary syscall used for burst testing; returns `E_RATE_LIMIT` when throttled |
| `SYS_DELAY` (1) | Sleep between phases and for the recovery window |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_TASK_ID` (3) | Print the task's ID at startup |
| `SYS_UPTIME` (4) | Available for timing (used in syscall stub) |

## Rate Limiter Design

The kernel enforces a per-task sliding window of **1000 syscalls per 1000 ms** (configurable via `os_cfg::SYSCALL_RATE_LIMIT` and `os_cfg::SYSCALL_RATE_WINDOW_MS`). When a task exceeds the limit within its current window, subsequent syscalls return `E_RATE_LIMIT` immediately without dispatching. The window resets as time advances, restoring normal operation.

This protects the kernel from denial-of-service by runaway or malicious user-space tasks.

## Sample Output

```
[rate-limit] syscall rate limit demo started (EL0)
[rate-limit] task = 5
[rate-limit] Rate limit: 1000 syscalls/second
[rate-limit] --- Phase 1: normal operation ---
[rate-limit]   Normal: 10 of 10 syscalls OK
[rate-limit] --- Phase 2: burst test ---
[rate-limit]   Burst: 412 succeeded, 188 rate-limited
[rate-limit] --- Phase 3: recovery ---
[rate-limit]   waiting 2s for window...
[rate-limit]   Recovery: syscalls working!
[rate-limit] cycle complete, waiting 30s...
```

## Notes

- The exact burst split between succeeded and rate-limited depends on how many syscalls the task has already consumed in the current window (including the `SYS_WRITE` and `SYS_DELAY` calls from Phase 1)
- On QEMU with `raspi4b`, the rate limiter runs against the virtual timer tick rate
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
- The `E_RATE_LIMIT` error code is distinct from other kernel errors, allowing user-space to detect and handle throttling gracefully
