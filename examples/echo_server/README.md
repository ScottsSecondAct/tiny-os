# UDP Echo Server

User-space EL0 application that creates a UDP socket, binds to a port, and echoes received packets back to the sender.

## What It Does

- Creates a UDP socket via `SYS_NET` (NET_SOCKET operation)
- Binds to port 7777 via `SYS_NET` (NET_BIND operation)
- Polls for incoming packets in a loop via `SYS_NET` (NET_RECV operation)
- Echoes any received data back to the sender via `SYS_NET` (NET_SEND operation)
- Tracks the total number of packets echoed
- Prints periodic status updates every 30 seconds (packet count and uptime)
- Handles errors gracefully: prints diagnostics and enters a yield loop on failure

## Syscalls Used

| Syscall | Purpose |
|---------|---------|
| `SYS_YIELD` (0) | Yield CPU when no data is available |
| `SYS_DELAY` (1) | Small poll delay to avoid busy-spinning |
| `SYS_WRITE` (2) | Print formatted output to the console |
| `SYS_UPTIME` (4) | Timestamp for periodic status reports |
| `SYS_NET` (11) | Socket create, bind, recv, send, close |

## NET Operations

| Operation | Code | Arguments | Returns |
|-----------|------|-----------|---------|
| `NET_SOCKET` | 0 | type (0=UDP) | socket fd or error |
| `NET_BIND` | 1 | fd, port | 0 on success |
| `NET_SEND` | 3 | fd, buf_ptr, buf_len | bytes sent |
| `NET_RECV` | 4 | fd, buf_ptr, buf_len | bytes received (0 if none) |
| `NET_CLOSE` | 5 | fd | 0 |

## Sample Output

```
[echo-srv] UDP echo server (EL0)
[echo-srv] socket created
[echo-srv] bind succeeded
[echo-srv] listening for data
[echo-srv] status: 0 pkts up=30000ms
[echo-srv] status: 0 pkts up=60000ms
```

When a packet is received and echoed:

```
[echo-srv] echoed 42 bytes
```

## Design

- Runs at standard user-space priority, suitable for background network services
- On QEMU, the loopback device provides a working network path; the echo server demonstrates the full socket lifecycle even without external traffic
- On real Pi 5 hardware with the RP1 Ethernet MAC, external UDP packets on port 7777 would be echoed back to their sender
- Error handling checks for `E_NOSYS` (syscall not available) and `E_PERM` (capability denied)
- All code and data are placed in the `.user.text` linker section for EL0 accessibility
- No heap allocation; all formatting uses fixed-size stack buffers with volatile writes
- Poll loop uses `sys_delay(10)` (10 ms) to balance responsiveness with CPU efficiency
