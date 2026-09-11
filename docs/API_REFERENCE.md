# tiny_os API Reference

Complete reference for all kernel APIs, syscalls, shell commands, and HAL traits.

> **Writing a user-space app?** See the [User-Space Application Developer's Guide](USER_APP_GUIDE.md) for a hands-on walkthrough with examples.

---

## Table of Contents

- [Shell Commands](#shell-commands)
- [User-Mode Syscalls](#user-mode-syscalls)
- [Scheduler](#scheduler)
- [Synchronization Primitives](#synchronization-primitives)
- [Filesystem](#filesystem)
- [Network Stack](#network-stack)
- [Memory Management](#memory-management)
- [Dynamic ELF Loader](#dynamic-elf-loader)
- [Network Buffer Pool](#network-buffer-pool)
- [Logging](#logging)
- [Watchdog](#watchdog)
- [Health Monitor](#health-monitor)
- [SpinLock](#spinlock)
- [HAL Traits](#hal-traits)

---

## Shell Commands

Interactive commands at the `tiny_os>` UART prompt. Type `help` for the built-in list.

| Command | Description |
|---------|-------------|
| `help` | Print list of available commands |
| `uptime` | Display system uptime in seconds and milliseconds |
| `ticks` | Print the raw system tick count |
| `info` | Show timer frequency, core count, and current core |
| `mem` | Page allocator stats, heap stats, netbuf pool stats, MMU status |
| `tasks` | List active tasks with ID, name, priority, state, criticality, CPU ticks |
| `smp` | Show the number of active SMP cores |
| `sd` | Display SD card info and partition table |
| `sdread <lba>` | Read and hex-dump a single sector at the given LBA |
| `ls [path]` | List directory contents (defaults to root) |
| `cat <path>` | Print the contents of a file |
| `hexdump <path>` | Hex-dump the contents of a file |
| `touch <path>` | Create an empty file |
| `write <path> <text>` | Write text to a file (creating it if needed) |
| `log [N]` | Show the last N log entries (default 16) |
| `log level <level>` | Set the minimum log level (error/warn/info/debug/trace) |
| `health` | Show stack watermarks, CPU utilization, and watchdog status |
| `ping <ip>` | Send an ICMP echo request and display the reply RTT |
| `netstat` | Show IP/MAC config, ARP cache, and open socket count |
| `ifconfig` | Show network interface configuration (loopback) |
| `temp` | Show the current SoC temperature (°C) |
| `exec <path>` | Load and execute an ELF64 binary from filesystem (requires `dynamic-load` feature) |
| `yield` | Yield the current task's timeslice |
| `svc` | Trigger a test SVC #42 exception |
| `reboot` | Reboot the system |

---

## User-Mode Syscalls

EL0 tasks invoke syscalls via `SVC #0`. The syscall number goes in **X8**, arguments in **X0-X3**, and the return value comes back in **X0**.

### Basic Syscalls

X8 selects the syscall; X0 and X1 carry arguments directly.

| # | Name | Args | Return | Description |
|---|------|------|--------|-------------|
| 0 | `SYS_YIELD` | — | — | Yield the current timeslice |
| 1 | `SYS_DELAY` | X0: milliseconds | — | Sleep for the given duration |
| 2 | `SYS_WRITE` | X0: pointer, X1: length | — | Write a byte buffer to the console |
| 3 | `SYS_TASK_ID` | — | X0: task ID | Return the current task's ID |
| 4 | `SYS_UPTIME` | — | X0: ticks | Return the system uptime in ticks |
| 5 | `SYS_EXIT` | — | (no return) | Terminate the current task |
| 6 | `SYS_TEMPERATURE` | — | X0: millidegrees C | Read SoC temperature via VideoCore mailbox |

### Subsystem Syscalls

X8 selects the subsystem; X0 selects the operation within it; X1-X3 carry arguments.

| X8 | Subsystem | Description |
|----|-----------|-------------|
| 10 | `SYS_FS` | Filesystem operations |
| 11 | `SYS_NET` | Network socket operations |
| 12 | `SYS_SPI` | SPI bus operations |
| 13 | `SYS_I2C` | I2C bus operations |
| 14 | `SYS_GPIO` | GPIO pin operations |

#### Error Codes

All subsystem syscalls return `u64::MAX` (`0xFFFF_FFFF_FFFF_FFFF`) family values on error:

| Value | Name | Meaning |
|-------|------|---------|
| `u64::MAX` | `E_NOSYS` | Operation not implemented |
| `u64::MAX-1` | `E_BADF` | Bad file/socket descriptor |
| `u64::MAX-2` | `E_INVAL` | Invalid argument |
| `u64::MAX-3` | `E_NOMEM` | Out of memory |
| `u64::MAX-4` | `E_IO` | I/O error |
| `u64::MAX-5` | `E_NOENT` | No such file or entry |
| `u64::MAX-6` | `E_NOSPC` | No space left |
| `u64::MAX-7` | `E_BUSY` | Resource busy |
| `u64::MAX-8` | `E_PERM` | Permission denied |

#### SYS_FS (10) — Filesystem

| X0 | Operation | X1 | X2 | X3 | Return |
|----|-----------|----|----|-----|--------|
| 0 | `FS_OPEN` | path_ptr | path_len | flags (0=RO, 1=RW) | fd or error |
| 1 | `FS_READ` | fd | buf_ptr | buf_len | bytes read or error |
| 2 | `FS_WRITE` | fd | buf_ptr | buf_len | bytes written or error |
| 3 | `FS_CLOSE` | fd | — | — | 0 or error |
| 4 | `FS_STAT` | path_ptr | path_len | out_ptr | 0 or error |
| 5 | `FS_CREATE` | path_ptr | path_len | — | fd or error |

#### SYS_NET (11) — Network

| X0 | Operation | X1 | X2 | X3 | Return |
|----|-----------|----|----|-----|--------|
| 0 | `NET_SOCKET` | type (0=UDP, 1=TCP) | — | — | fd or error |
| 1 | `NET_BIND` | fd | port | — | 0 or error |
| 2 | `NET_CONNECT` | fd | ipv4 (BE u32) | port | 0 or error |
| 3 | `NET_SEND` | fd | data_ptr | data_len | bytes sent or error |
| 4 | `NET_RECV` | fd | buf_ptr | buf_len | bytes received or error |
| 5 | `NET_CLOSE` | fd | — | — | 0 |

#### SYS_SPI (12) — SPI Bus

| X0 | Operation | X1 | X2 | X3 | Return |
|----|-----------|----|----|-----|--------|
| 0 | `SPI_OPEN` | clock_hz | mode (0-3) | cs_pin | 0 or error |
| 1 | `SPI_TRANSFER` | tx_ptr | rx_ptr | len | bytes transferred or error |
| 2 | `SPI_CLOSE` | — | — | — | 0 |

#### SYS_I2C (13) — I2C Bus

| X0 | Operation | X1 | X2 | X3 | Return |
|----|-----------|----|----|-----|--------|
| 0 | `I2C_OPEN` | clock_hz | — | — | 0 or error |
| 1 | `I2C_READ` | device_addr | buf_ptr | buf_len | bytes read or error |
| 2 | `I2C_WRITE` | device_addr | buf_ptr | buf_len | bytes written or error |
| 3 | `I2C_CLOSE` | — | — | — | 0 |

#### SYS_GPIO (14) — GPIO Pins

| X0 | Operation | X1 | X2 | X3 | Return |
|----|-----------|----|----|-----|--------|
| 0 | `GPIO_SET_MODE` | pin | mode (0=In, 1=Out, 2-7=Alt0-5) | — | 0 or error |
| 1 | `GPIO_READ` | pin | — | — | 0 or 1, or error |
| 2 | `GPIO_WRITE` | pin | value (0/1) | — | 0 or error |
| 3 | `GPIO_SET_PULL` | pin | pull (0=None, 1=Up, 2=Down) | — | 0 or error |

### Inline-asm calling convention

```rust
// 4-argument subsystem syscall wrapper
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
```

---

## Scheduler

Module: `kernel::sched` (`kernel/src/sched.rs`)

256-level fixed-priority preemptive scheduler with SMP support (4 cores), per-core idle tasks, IPI-triggered reschedule, and TTBR0 swap for user-mode tasks.

### Task Lifecycle

```rust
pub fn task_create(
    name: &'static str,
    priority: u8,
    criticality: Criticality,
    stack: &'static mut [u8],
    entry: fn(usize) -> !,
    arg: usize,
) -> Result<u8, &'static str>
```
Create a kernel-mode task. Returns the task ID (0-15).

```rust
pub fn task_create_user(
    name: &'static str,
    priority: u8,
    criticality: Criticality,
    kernel_stack: &'static mut [u8],
    user_entry: usize,
    user_stack_top: usize,
    arg: usize,
    ttbr0: u64,
) -> Result<u8, &'static str>
```
Create an EL0 user-mode task with its own TTBR0 page table. Returns the task ID.

```rust
pub fn task_delete(id: u8) -> Result<(), &'static str>
pub fn task_suspend(id: u8) -> Result<(), &'static str>
pub fn task_resume(id: u8) -> Result<(), &'static str>
pub fn task_terminate(id: u8)
```

### Scheduling

```rust
pub fn task_yield()                         // Yield current timeslice
pub fn delay(ticks: u32)                    // Block for N timer ticks
pub fn start() -> !                         // Start scheduler on primary core
pub fn start_secondary(core: usize) -> !    // Start scheduler on secondary core
pub fn tick()                               // Called from timer ISR each tick
pub fn ipi_reschedule()                     // Handle IPI reschedule interrupt
```

### Blocking (used by sync primitives)

```rust
pub fn block_current()                      // Block indefinitely
pub fn block_current_timeout(ticks: u32)    // Block with timeout (0 = infinite)
pub fn wake_task(id: u8)                    // Wake a blocked task
pub fn get_wait_result() -> WaitResult      // Ok or Timeout
pub fn set_task_wait_result(id: u8, result: WaitResult)
```

### Priority

```rust
pub fn get_priority(id: u8) -> u8           // Effective (possibly inherited) priority
pub fn get_base_priority(id: u8) -> u8      // Base priority
pub fn set_priority(id: u8, prio: u8)       // Set effective priority (PIP/PCP)
```

### Introspection

```rust
pub fn current_id() -> u8                   // Current task ID (with lock)
pub fn current_task_id() -> u8              // Current task ID (no lock)
pub fn current_task_name() -> &'static str
pub fn get_state(id: u8) -> TaskState       // Ready, Running, Blocked, Suspended, Dormant
pub fn task_count() -> u8                   // Number of active tasks
pub fn active_cores() -> u8                 // Number of cores running scheduler
pub fn task_list() -> [(u8, &'static str, u8, TaskState); MAX_TASKS]
pub fn task_list_ext() -> [(u8, &'static str, u8, TaskState, Criticality, u32, u64); MAX_TASKS]
pub fn task_stack_info() -> [(u8, &'static str, usize, usize); MAX_TASKS]
pub fn utilization() -> (u64, u64)          // (busy_ticks, total_ticks)
pub fn tick_count_32() -> u32
```

### Budget & Criticality

```rust
pub fn task_set_budget(id: u8, ticks: u32)
pub fn task_get_remaining(id: u8) -> u32
pub fn task_reset_budget(id: u8)
pub fn task_set_criticality(id: u8, crit: Criticality)
```

### Criticality Levels

```rust
pub enum Criticality {
    SafetyCritical,    // IEC 61508 SIL-2 / ISO 26262 ASIL-B
    MissionCritical,
    Standard,
    BestEffort,
}
```

---

## Synchronization Primitives

### Mutex (`kernel::sync::mutex`)

Supports Priority Inheritance (PIP), Priority Ceiling (PCP), and recursive locking (max depth 8).

```rust
pub const fn new(protocol: MutexProtocol) -> Self
pub fn lock(&self) -> Result<(), &'static str>
pub fn lock_timeout(&self, ticks: u32) -> Result<(), &'static str>
pub fn try_lock(&self) -> bool
pub fn unlock(&self) -> Result<(), &'static str>
pub fn owner(&self) -> Option<u8>
```

```rust
pub enum MutexProtocol { None, PIP, PCP(u8) }  // PCP takes ceiling priority
```

### Semaphore (`kernel::sync::semaphore`)

Counting and binary semaphores with timeout support.

```rust
pub const fn new(init: u32, max: u32) -> Self
pub const fn binary(init: u32) -> Self          // Shorthand for max=1
pub fn wait(&self) -> Result<(), &'static str>
pub fn wait_timeout(&self, ticks: u32) -> Result<(), &'static str>
pub fn try_wait(&self) -> bool
pub fn post(&self) -> Result<(), &'static str>
pub fn count(&self) -> u32
```

### EventFlags (`kernel::sync::events`)

32-bit event flag group with Any/All wait modes, up to 16 concurrent waiters.

```rust
pub const fn new() -> Self
pub fn set(&self, bits: u32)
pub fn clear(&self, bits: u32)
pub fn get(&self) -> u32
pub fn wait(&self, mask: u32, mode: EventWaitMode) -> Result<u32, &'static str>
pub fn wait_timeout(&self, mask: u32, mode: EventWaitMode, ticks: u32) -> Result<u32, &'static str>
```

```rust
pub enum EventWaitMode { Any, All }
```

### MsgQueue (`kernel::sync::msgqueue`)

Const-generic message queue with fixed-size messages and circular buffer.

```rust
// MsgQueue<const MSG_SIZE: usize, const CAPACITY: usize>
pub const fn new() -> Self
pub fn send(&self, msg: &[u8; MSG_SIZE]) -> Result<(), &'static str>
pub fn send_timeout(&self, msg: &[u8; MSG_SIZE], ticks: u32) -> Result<(), &'static str>
pub fn try_send(&self, msg: &[u8; MSG_SIZE]) -> bool
pub fn recv(&self, buf: &mut [u8; MSG_SIZE]) -> Result<(), &'static str>
pub fn recv_timeout(&self, buf: &mut [u8; MSG_SIZE], ticks: u32) -> Result<(), &'static str>
pub fn try_recv(&self, buf: &mut [u8; MSG_SIZE]) -> bool
pub fn count(&self) -> usize
```

### WaitQueue (`kernel::sync::mod`)

Priority-ordered waiter array used internally by all sync primitives.

```rust
pub const fn new() -> Self
pub fn add(&mut self, id: u8)
pub fn remove(&mut self, id: u8)
pub fn pop_highest(&mut self) -> Option<u8>
pub fn highest_blocked_priority(&self) -> Option<u8>
pub fn is_empty(&self) -> bool
pub fn cleanup_stale(&mut self)
```

---

## Filesystem

Module: `kernel::fs` (`kernel/src/fs/mod.rs`)

FAT32 filesystem with VFS abstraction layer, 16-entry file descriptor table, LFN support.

### Initialization

```rust
pub fn init() -> Result<(), FsError>        // Mount FAT32 from first partition
pub fn is_mounted() -> bool
pub fn cluster_count() -> u32
```

### File Operations

```rust
pub fn open(path: &str, writable: bool) -> Result<usize, FsError>  // Returns fd
pub fn create(path: &str) -> Result<usize, FsError>                 // Create or open for writing
pub fn read(fd: usize, buf: &mut [u8]) -> Result<usize, FsError>   // Returns bytes read
pub fn write(fd: usize, buf: &[u8]) -> Result<usize, FsError>      // Returns bytes written
pub fn close(fd: usize) -> Result<(), FsError>                      // Flush + release fd
pub fn stat(path: &str) -> Result<DirEntry, FsError>
```

### Directory Iteration

```rust
pub fn readdir_open(path: &str) -> Result<usize, FsError>           // Returns cursor fd
pub fn readdir_next(fd: usize) -> Result<Option<DirEntry>, FsError>  // None when exhausted
pub fn readdir_close(fd: usize) -> Result<(), FsError>
```

### Error Type

```rust
pub enum FsError {
    NotMounted, NotFound, NoFreeDescriptors, InvalidDescriptor,
    NotWritable, DiskFull, IoError, InvalidPath, AlreadyExists,
}
```

---

## Network Stack

### Socket API (`kernel::net::socket`)

BSD-style sockets over UDP and TCP. 8-entry socket table.

```rust
pub fn socket(sock_type: SockType) -> Result<u8, NetError>  // Returns fd
pub fn bind(fd: u8, port: u16) -> Result<(), NetError>
pub fn connect(fd: u8, addr: Ipv4Addr, port: u16) -> Result<(), NetError>
pub fn sendto(fd: u8, data: &[u8], addr: Ipv4Addr, port: u16) -> Result<usize, NetError>
pub fn send(fd: u8, data: &[u8]) -> Result<usize, NetError>
pub fn recvfrom(fd: u8, buf: &mut [u8]) -> Result<(usize, Ipv4Addr, u16), NetError>
pub fn close(fd: u8)
pub fn socket_count() -> usize
```

```rust
pub enum SockType { Udp, Tcp }
pub enum NetError { NoLink, QueueFull, InvalidBuf, NotFound, InUse, ConnRefused }
```

### Network Core (`kernel::net`)

```rust
pub fn init(ip: Ipv4Addr, mac: [u8; 6])    // Init stack with IP and MAC
pub fn our_ip() -> Ipv4Addr
pub fn our_mac() -> [u8; 6]
pub fn process_rx(buf_idx: u16)             // Process received Ethernet frame
pub fn net_task(_arg: usize) -> !           // Network polling task entry
```

### ICMP (`kernel::net::icmp`)

```rust
pub fn send_ping(dst: Ipv4Addr) -> u16             // Returns sequence number
pub fn ping_result(expected_seq: u16) -> Option<u32> // Returns RTT in ticks
pub fn process_rx(buf_idx: u16, ip_hdr: &Ipv4Header)
```

### ARP (`kernel::net::arp`)

16-entry cache with request/reply handling.

```rust
pub fn init()
pub fn insert(ip: Ipv4Addr, mac: [u8; 6])
pub fn resolve(ip: Ipv4Addr) -> Option<[u8; 6]>
pub fn cache_entries() -> &'static [ArpEntry]
pub fn send_request(target_ip: Ipv4Addr)
pub fn process_rx(buf_idx: u16)
```

### UDP (`kernel::net::udp`)

```rust
pub fn bind(port: u16) -> Option<u8>                        // Returns handle
pub fn unbind(handle: u8)
pub fn send(dst: Ipv4Addr, dst_port: u16, src_port: u16, payload: &[u8])
pub fn recv(handle: u8) -> Option<(u16, Ipv4Addr, u16)>     // (buf_idx, src_ip, src_port)
pub fn process_rx(buf_idx: u16, ip_hdr: &Ipv4Header)
```

### TCP (`kernel::net::tcp`)

Minimal client-only state machine (SYN/ACK/FIN). 4 concurrent connections.

```rust
pub fn connect(dst: Ipv4Addr, dst_port: u16) -> Result<u8, NetError>  // Returns handle
pub fn send_data(handle: u8, data: &[u8]) -> Result<usize, NetError>
pub fn recv_data(handle: u8, buf: &mut [u8]) -> Result<usize, NetError>
pub fn close(handle: u8)
pub fn conn_state(handle: u8) -> bool       // true if Established
pub fn process_rx(buf_idx: u16, ip_hdr: &Ipv4Header)
```

### IPv4 (`kernel::net::ipv4`)

```rust
pub fn parse_header(data: &[u8]) -> Option<Ipv4Header>
pub fn checksum(data: &[u8]) -> u16         // Internet checksum (one's complement)
pub fn send(buf_idx: u16, dst: Ipv4Addr, protocol: u8, payload_len: u16)
pub fn process_rx(buf_idx: u16)
```

### Ethernet (`kernel::net::ethernet`)

```rust
pub fn parse_ethertype(data: &[u8]) -> u16
pub fn parse_header(data: &[u8]) -> Option<EthHeader>
pub fn strip_header(buf: &mut NetBuf)
pub fn prepend_header(buf: &mut NetBuf, dst_mac: &[u8; 6], src_mac: &[u8; 6], ethertype: u16) -> bool
pub fn send_frame(buf_idx: u16, dst_mac: &[u8; 6], ethertype: u16)
```

### Loopback Device (`kernel::net::loopback`)

Software loopback for QEMU testing. Swaps src/dst addresses and converts ICMP echo requests to replies.

```rust
pub fn init(mac: [u8; 6])
pub fn device() -> &'static mut dyn NetDevice
```

---

## Memory Management

### Init (`kernel::mm`)

```rust
pub fn init()                                   // DTB → PMM → MMU → heap → DMA pool
pub fn page_stats() -> (usize, usize, usize)    // (total, used, free) pages
pub fn heap_stats() -> (usize, usize, usize)    // (total, used, free) bytes
```

### Page Frame Allocator (`kernel::mm::pmm`)

Bitmap-based, 4KB pages, up to 4GB (1M pages).

```rust
pub fn init(&mut self, _ram_base: usize, ram_size: usize)
pub fn mark_range_used(&mut self, base: usize, size: usize)
fn alloc_page(&mut self) -> Option<usize>           // Single 4KB page
fn alloc_pages(&mut self, count: usize) -> Option<usize>  // Contiguous run
fn free_page(&mut self, pa: usize)
fn total_pages(&self) -> usize
fn used_pages(&self) -> usize
```

### Heap Allocator (`kernel::mm::heap`)

Linked-list allocator seeded with 256KB from PMM.

```rust
pub unsafe fn add_region(base: usize, size: usize)
pub fn kmalloc(size: usize, align: usize) -> Option<*mut u8>
pub fn kfree(ptr: *mut u8)
pub fn stats() -> (usize, usize, usize)   // (total, used, free) bytes
```

### MMU (`arch::aarch64::mmu`)

Identity mapping with 4KB granule, 2MB block descriptors, W^X policy.

```rust
pub unsafe fn init(regions: &[MemRegion])       // Primary core MMU setup
pub unsafe fn init_secondary()                  // Secondary core MMU setup
pub fn enabled() -> bool
pub fn kernel_ttbr0() -> u64                    // Kernel page table base
pub unsafe fn create_user_page_table(           // Per-task page table for EL0
    user_code_base: usize,
    user_code_size: usize,
    user_stack_base: usize,
    user_stack_pages: usize,
) -> u64                                        // Returns TTBR0 with ASID
pub unsafe fn free_user_page_table(ttbr0: u64)
pub unsafe fn switch_ttbr0(ttbr0: u64)          // Swap TTBR0 + TLBI + DSB + ISB
```

```rust
pub enum MemKind { RoCode, Ram, NonCacheable, Device }
```

---

## Dynamic ELF Loader

Module: `kernel::loader` (`kernel/src/loader.rs`) — requires `dynamic-load` Cargo feature.

Loads ELF64 PIE binaries from the filesystem at runtime, creating EL0 user-mode tasks. Disabled by default for safety-critical builds where static linking is required.

### Build

```sh
# Enable dynamic loading
cargo build --features kernel/dynamic-load

# Default (no dynamic loading, production/certification)
cargo build
```

### Shell Usage

```
exec /apps/hello.elf
```

### API

```rust
pub fn load_and_exec(path: &str) -> Result<u8, LoadError>
```

Parses the ELF64 header, loads PT_LOAD segments into PMM-allocated pages, applies R_AARCH64_RELATIVE relocations for PIE binaries, creates per-task page tables with W^X permissions (RX for code, RW for data/stack), and spawns the task at EL0.

Returns the new task ID on success.

### Supported Binary Format

- ELF64 AArch64 (`EM_AARCH64`)
- Position-Independent Executable (PIE, `ET_DYN`)
- R_AARCH64_RELATIVE relocations
- Up to 8 PT_LOAD segments

---

## Network Buffer Pool

Module: `kernel::netbuf` (`kernel/src/netbuf.rs`)

1024 x 1536-byte buffers in a 2MB non-cacheable memory region for zero-copy DMA I/O.

### Pool Management

```rust
pub fn init()                                   // Allocate and initialize the pool
pub fn alloc() -> Option<&'static mut NetBuf>   // Allocate a buffer
pub fn free(buf: &mut NetBuf)                   // Free (refcount-based)
pub fn get(idx: u16) -> Option<&'static mut NetBuf>  // Get by index
pub fn buf_index(buf: &NetBuf) -> u16           // Get index of buffer
pub fn pool_stats() -> (u16, u16)               // (total, free)
```

### NetBuf Methods

```rust
pub fn len(&self) -> usize
pub fn as_slice(&self) -> &[u8]
pub fn as_mut_slice(&mut self) -> &mut [u8]
pub fn push_data(&mut self, src: &[u8]) -> bool      // Append to tail
pub fn prepend_header(&mut self, hdr: &[u8]) -> bool  // Prepend before head
pub fn reset(&mut self)                                // Reset head/tail
pub fn dma_addr(&self) -> usize                       // DMA-visible address
```

---

## Logging

Module: `kernel::klog` (`kernel/src/klog.rs`)

5-level ring-buffer log with timestamps and module tags. 64-entry buffer.

```rust
pub fn set_level(level: LogLevel)
pub fn get_level() -> LogLevel
pub fn log(level: LogLevel, module: &str, args: fmt::Arguments)
pub fn dump(count: usize)              // Print last N entries to console
pub fn entry_count() -> usize
```

```rust
pub enum LogLevel { Error, Warn, Info, Debug, Trace }
```

Macros: `klog!`, `klog_error!`, `klog_warn!`, `klog_info!`, `klog_debug!`, `klog_trace!`

---

## Watchdog

Module: `kernel::watchdog` (`kernel/src/watchdog.rs`)

Software watchdog with tick-based counter. Auto-kick task runs at priority 0.

```rust
pub fn init(timeout_ms: u32)    // Enable with timeout
pub fn kick()                   // Reset counter
pub fn tick()                   // Called from sched::tick() in ISR
pub fn is_enabled() -> bool
pub fn counter() -> u32
pub fn timeout() -> u32         // Configured timeout in ticks
pub fn kick_count() -> u64      // Total kicks since init
```

---

## Health Monitor

Module: `kernel::health` (`kernel/src/health.rs`)

Periodic task (priority 1) that checks stack watermarks, CPU utilization, and watchdog status every 5 seconds.

```rust
pub fn health_task(_arg: usize) -> !    // Task entry point
```

---

## SpinLock

Module: `kernel::spinlock` (`kernel/src/spinlock.rs`)

Ticket spinlock with IRQ save/restore for SMP mutual exclusion.

```rust
pub const fn new() -> Self
pub fn lock(&self) -> u64               // Returns saved DAIF
pub fn unlock(&self, saved_daif: u64)   // Restore IRQ state
pub unsafe fn force_unlock(&self)       // No DAIF restore (for task_trampoline)
```

---

## HAL Traits

Hardware abstraction traits in the `arch` crate. Implement these to port to a new architecture.

| Trait | Module | Methods |
|-------|--------|---------|
| `UartDriver` | `arch::uart` | `init()`, `putc(u8)`, `getc() -> Option<u8>` |
| `InterruptController` | `arch::irq` | `init()`, `enable(id)`, `disable(id)`, `set_priority(id, prio)` |
| `Timer` | `arch::timer` | `init(freq)`, `enable()`, `disable()`, `frequency() -> u64` |
| `PageAllocator` | `arch::mm` | `alloc_page()`, `alloc_pages(n)`, `free_page(pa)`, `total_pages()`, `used_pages()` |
| `Context` | `arch::context` | `new_context(entry, stack, arg) -> u64`, `switch(old, new)` |
| `SmpBoot` | `arch::smp` | `core_id() -> usize`, `num_cores() -> usize`, `start_core(id, entry)` |
| `DmaEngine` | `arch::dma` | `configure(ch, src, dst, len)`, `start(ch)`, `complete(ch) -> bool`, `abort(ch)` |
| `BlockDevice` | `arch::block` | `read_block(lba, buf)`, `write_block(lba, buf)`, `block_count()`, `block_size()` |
| `NetDevice` | `arch::net` | `send(buf_idx) -> Result`, `recv() -> Option<u16>`, `mac_addr() -> [u8; 6]` |
| `UserContext` | `arch::user` | `new_user_context(entry, user_sp, arg, kernel_sp) -> u64` |
| `SpiDevice` | `arch::spi` | `configure(config)`, `transfer(tx, rx)`, `write(data)`, `read(buf)` |
| `I2cDevice` | `arch::i2c` | `configure(config)`, `write(addr, data)`, `read(addr, buf)`, `write_read(addr, tx, rx)` |
| `GpioController` | `arch::gpio` | `set_mode(pin, mode)`, `set_pull(pin, pull)`, `read(pin)`, `write(pin, high)` |
