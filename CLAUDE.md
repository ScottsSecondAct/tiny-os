# tiny_os — Project Context for Claude Code

## What This Is

tiny_os is a bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). It is designed for portability to other ARM cores (Cortex-A and Cortex-M families). The specification (v1.2) includes RTOS certification provisions for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C — covering WCET bounds, MC/DC coverage, health monitoring, watchdog integration, structured shutdown, mixed-criticality partitioning, and requirements traceability. The full specifications and implementation phases are in `docs/`.

## Current Phase

**Phase 10: Networking & User Mode** — complete. Zero-copy network stack with loopback device for QEMU testing: Ethernet, ARP, IPv4, ICMP, UDP, TCP (minimal client state machine), BSD socket API. EL0 user-mode task support: per-task TTBR0 page tables (L3 4KB granularity with guard pages), ASID-tagged address spaces, `task_trampoline_user` (eret to EL0), SVC-based syscall dispatch (yield, delay, write, task_id, uptime, exit). Shell commands: `ping`, `netstat`, `ifconfig`. User demo task runs at EL0 printing via syscalls. Next up: Phase 11 (Safety Certification).

## Target Hardware

- **Primary:** Raspberry Pi 5 (BCM2712, 4× Cortex-A76 @ 2.4 GHz, GIC-400)
- **Also compatible:** Raspberry Pi 500, 500+, Compute Module 5
- **SoC stepping:** BCM2712 D0 (shipping on 1/2/16 GB boards, transitioning 4/8 GB)
- **Southbridge:** RP1 — connected via PCIe x4; owns UART, GPIO, SPI, I²C, Ethernet
- **DRAM variants:** 1 GB, 2 GB, 4 GB, 8 GB, 16 GB
- **QEMU testing:** Use `-M raspi4b` (closest available; no raspi5 machine yet)

## Key Hardware Details

- **Kernel load address:** `0x80000` (RPi firmware convention)
- **RP1 peripheral window:** Physical address `0x1F_0000_0000`, maps to RP1 internal `0x4000_0000`
- **GIC-400:** Standard ARM GICv2 interrupt controller
- **Timer:** ARM Generic Timer (virtual timer CNTV_*_EL0 on QEMU, physical CNTP_*_EL0 on Pi 5), frequency from CNTFRQ_EL0
- **Device tree:** Firmware passes DTB at boot; use for memory/peripheral discovery

### Critical config.txt Settings (bare metal)

```
os_check=0            # Disable firmware OS compatibility check
uart_early_init=1     # Firmware pre-initializes RP1 UART0 @ 115200 baud, preserves PCIe link
pciex4_reset=0        # Don't reset PCIe x4 controller; inherit working RP1 link
```

## Build Target & Toolchain

- **Rust target triple:** `aarch64-unknown-none` (bare-metal, no_std, no_main)
- **Toolchain:** nightly (required for inline assembly, naked functions, global_asm)
- **Required components:** `rust-src`, `llvm-tools`
- **Binary output:** `kernel8.img` (raw binary via `cargo objcopy -O binary`)
- **Cross tools:** `gcc-aarch64-linux-gnu`, `binutils-aarch64-linux-gnu` (for linking, objdump)

## Architecture & Portability Rules

All hardware-specific code is isolated behind Rust traits so porting requires implementing a bounded set of trait impls, not rewriting the kernel. The key rule: **if it touches a hardware register, it goes in `arch/` or `bsp/`, never in `kernel/`**.

### HAL Trait Summary

| Trait                | Module           | Phase | Purpose                              |
|----------------------|------------------|-------|--------------------------------------|
| `UartDriver`         | `arch::uart`     | 1     | Serial I/O                           |
| `InterruptController`| `arch::irq`      | 2     | GIC / NVIC abstraction               |
| `Timer`              | `arch::timer`    | 2     | Periodic tick, monotonic clock        |
| `PageAllocator`      | `arch::mm`       | 3     | Physical page frame management        |
| `Context`            | `arch::context`  | 4     | Task context save/restore/switch      |
| `SmpBoot`            | `arch::smp`      | 7     | Multi-core startup                   |
| `DmaEngine`          | `arch::dma`      | 8     | DMA transfers                        |
| `BlockDevice`        | `drivers::block`  | 8     | Sector read/write                    |
| `NetDevice`          | `drivers::net`    | 10    | Packet TX/RX                         |
| `UserContext`         | `arch::user`     | 10    | EL0 task isolation, syscalls          |

## Cargo Workspace Layout

```
tiny_os/
├── Cargo.toml              # Workspace root
├── CLAUDE.md               # This file
├── docs/                   # Specifications (markdown)
│   ├── spec.md             # tiny_os system specification v1.1
│   ├── scheduler_spec.md   # Scheduler subsystem specification
│   └── phases.md           # Implementation phases breakdown
├── kernel/                 # Main kernel binary crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs         # kmain() entry point
│   │   ├── panic.rs        # panic_handler
│   │   ├── print.rs        # kprint!() / kprintln!() macros
│   │   ├── exceptions.rs   # IRQ dispatch, sync/SVC handler, unhandled trap
│   │   ├── shell.rs        # Interactive UART shell (help, uptime, ticks, info, mem, tasks, log, health, smp, sd, sdread, ls, cat, hexdump, touch, write, ping, netstat, ifconfig, yield, svc, reboot)
│   │   ├── netbuf.rs       # Zero-copy DMA buffer pool: 1024×1536B buffers in NC memory
│   │   ├── syscall.rs      # Syscall dispatch: SYS_YIELD, SYS_DELAY, SYS_WRITE, SYS_TASK_ID, SYS_UPTIME, SYS_EXIT
│   │   ├── user_tasks.rs   # EL0 user demo task with inline-asm syscall stubs (.user.text section)
│   │   ├── net/            # Network stack subsystem
│   │   │   ├── mod.rs      # Network init, RX dispatch, net_task poll loop
│   │   │   ├── ethernet.rs # Ethernet frame parse/build (14-byte header, EtherType)
│   │   │   ├── arp.rs      # ARP cache (16 entries) + request/reply handling
│   │   │   ├── ipv4.rs     # IPv4 parse/build (20-byte header), internet checksum
│   │   │   ├── icmp.rs     # ICMP echo request/reply, ping RTT tracking
│   │   │   ├── udp.rs      # UDP parse/build (8-byte header), port table
│   │   │   ├── tcp.rs      # Minimal TCP state machine (client SYN/ACK/FIN, 4 connections)
│   │   │   ├── socket.rs   # BSD socket API: socket/bind/connect/sendto/recvfrom/close
│   │   │   └── loopback.rs # Loopback NetDevice for QEMU (swaps src/dst, ICMP req→reply)
│   │   ├── fs/             # Filesystem subsystem
│   │   │   ├── mod.rs      # VFS: FsError, 16-entry fd table, open/read/write/close/readdir API
│   │   │   └── fat32.rs    # FAT32: BPB, FAT chain, dir parsing, LFN, read/write, create
│   │   ├── storage/        # Storage subsystem
│   │   │   ├── mod.rs      # Device routing (EMMC2/RamDisk), cached read/write, flush
│   │   │   ├── ramdisk.rs  # RAM-backed BlockDevice (256 KB, FAT32-formatted for QEMU)
│   │   │   ├── mbr.rs      # MBR partition table parser
│   │   │   └── cache.rs    # LRU write-back block cache (32 lines, 512B each)
│   │   ├── sched.rs        # SMP-aware 256-level fixed-priority scheduler: per-core current, spinlock, IPI, TTBR0 swap
│   │   ├── spinlock.rs     # Ticket spinlock with IRQ save/restore for SMP mutual exclusion
│   │   ├── klog.rs         # Ring-buffer log subsystem: 5 levels, timestamps, module tags, 64-entry buffer
│   │   ├── watchdog.rs     # Software watchdog: tick-based counter, auto-kick task at priority 0
│   │   ├── health.rs       # Health monitor task: stack watermarks, CPU utilization, watchdog status
│   │   ├── drivers.rs      # Driver trait (probe/remove lifecycle) and static registry
│   │   ├── sync/           # Synchronization primitives subsystem
│   │   │   ├── mod.rs      # WaitQueue: priority-sorted waiter array with lazy cleanup
│   │   │   ├── mutex.rs    # Mutex with PIP, PCP, recursive locking, timeout
│   │   │   ├── semaphore.rs # Counting/binary semaphore with timeout
│   │   │   ├── events.rs   # 32-bit event flags with Any/All wait modes
│   │   │   └── msgqueue.rs # Const-generic message queue with send/recv blocking
│   │   └── mm/             # Memory management subsystem
│   │       ├── mod.rs      # MM init: RAM discovery, PMM, DMA pool, MMU enable, heap seeding
│   │       ├── dtb.rs      # Minimal FDT parser for /memory node
│   │       ├── pmm.rs      # Bitmap page frame allocator (4KB pages, up to 4GB)
│   │       └── heap.rs     # Linked-list heap allocator (kmalloc/kfree)
│   └── link.ld             # Linker script (.text.boot, .text, .rodata, .user.text at 0x200000, .data, .bss, .stack)
├── arch/                   # Architecture-specific crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── uart.rs         # UartDriver trait
│       ├── irq.rs          # InterruptController trait
│       ├── timer.rs        # Timer trait
│       ├── mm.rs           # PageAllocator trait
│       ├── context.rs      # Context HAL trait (new_context, switch)
│       ├── smp.rs          # SmpBoot HAL trait (core_id, num_cores, start_core)
│       ├── block.rs        # BlockDevice HAL trait (sector read/write)
│       ├── dma.rs          # DmaEngine HAL trait (channel-based DMA transfers)
│       ├── net.rs          # NetDevice HAL trait (packet TX/RX)
│       ├── user.rs         # UserContext HAL trait (EL0 task isolation)
│       └── aarch64/
│           ├── mod.rs
│           ├── boot.S      # _start, spin-table secondary parking, secondary_boot EL drop
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries)
│           ├── exceptions.rs # TrapFrame, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 driver: distributor, CPU interface, SGI for IPI
│           ├── timer.rs    # ARM Generic Timer (virtual timer, 1kHz tick, secondary init)
│           ├── mmu.rs      # MMU: identity mapping, 2MB blocks, W^X, secondary core init,
│           │               #   per-task TTBR0 page tables (L3 4KB), ASID, switch_ttbr0
│           ├── smp.rs      # AArch64 SMP: spin-table wakeup, core_id, start_core
│           ├── emmc2.rs    # SDHCI/EMMC2 SD card driver: PIO mode, card init, read/write
│           ├── context.rs  # Aarch64Context: new_context (fake frame), new_user_context, switch wrapper
│           └── context_switch.S  # Context switch (x19-x30), task_trampoline (sched lock release),
│                           #   task_trampoline_user (eret to EL0)
└── bsp/                    # Board Support Packages
    ├── Cargo.toml
    └── src/
        ├── lib.rs          # cfg-gated re-exports (PlatformUart, GIC bases)
        ├── rpi5/
        │   ├── mod.rs
        │   ├── rp1_uart.rs
        │   └── memory_map.rs   # RP1 UART, GIC, RAM, peripheral + RP1 MMIO regions
        └── qemu_virt/
            ├── mod.rs
            ├── uart.rs     # BCM2711 PL011 UART at 0xFE20_1000
            └── memory_map.rs   # UART, GIC, RAM, peripheral MMIO regions
```

## Scheduler Design (for reference in Phase 4+)

- **256-level fixed-priority preemptive** with bitmap + CLZ for O(1) dispatch
- **Round-robin** among equal-priority tasks via per-level FIFO queues
- **Five task states:** Ready, Running, Blocked, Suspended, Dormant
- **EDF mode** available as alternative (min-heap based)
- **Priority inversion protection:** PIP and PCP, configurable per-mutex
- **Context switch budget:** < 1 µs on Cortex-A76
- **TCB fields:** 20 fields including task ID, priority, state, saved context, stack base/size, timing stats
- **Critical sections:** DAIF masking with nesting count (single-core); spin-locks for SMP (Phase 7+)

## Conventions

- **Naming:** Snake_case for Rust, uppercase for constants and config (`OS_CFG_*`)
- **Error handling:** Return `Result<T, OsError>` from all kernel APIs; `OsError` is an enum
- **No unwinding:** `panic = "abort"` in Cargo.toml; panics print and halt
- **Unsafe discipline:** Minimize `unsafe`; every `unsafe` block gets a `// SAFETY:` comment explaining the invariant
- **Assembly:** Use Rust `global_asm!()` for boot code and vector tables; inline `asm!()` for short sequences
- **MMIO access:** Always via `core::ptr::read_volatile` / `write_volatile`, wrapped in typed register structs
- **Logging:** Use `kprintln!()` for early boot; transition to `klog` subsystem in Phase 6

## Completed Phases

### Phase 1 — Bare-Metal Bootstrap & UART Console ✅

- [x] Cargo workspace with `kernel`, `arch`, `bsp` crates
- [x] Linker script: kernel at 0x80000, sections: .text, .rodata, .data, .bss, .stack
- [x] `_start` in AArch64 assembly: park secondary cores (WFE), zero .bss, set SP, branch to kmain
- [x] RP1 UART driver (MMIO volatile writes to RP1 peripheral window)
- [x] `kprint!()` / `kprintln!()` macros via `core::fmt::Write`
- [x] `panic_handler` that prints message + location, then infinite WFE
- [x] Build pipeline: `cargo build` → `cargo objcopy` → `kernel8.img`
- [x] Boot test on QEMU (`-M raspi4b -serial stdio`) and real Pi 5 hardware

### Phase 2 — Interrupts & Timer ✅

- [x] Exception vector table (`vectors.S`) — 2KB aligned, 16 entries with TrapFrame save/restore
- [x] GIC-400 driver: distributor + CPU interface init, IRQ enable/disable/priority, EOI
- [x] `InterruptController` HAL trait
- [x] ARM Generic Timer driver — virtual timer (CNTV) with CVAL-based acknowledge for accurate 1kHz tick
- [x] `Timer` HAL trait with periodic tick and monotonic clock
- [x] System tick ISR incrementing a global tick counter (AtomicU64)
- [x] Sync exception handler — SVC detection with ELR advance, ESR decoding
- [x] Interactive shell: help, uptime, ticks, info, svc, reboot
- [x] Boot: EL3→EL1 (secure) on QEMU, EL2→EL1 on real Pi 5
- [x] Verified: 250 ticks in 250ms (±0.4%), uptime accurate to wall-clock

### Phase 3 — Memory Management ✅

- [x] Minimal FDT (DTB) parser for /memory node RAM discovery
- [x] Boot.S preserves firmware DTB pointer (x19 → DTB_PTR global)
- [x] Physical page frame allocator: bitmap-based, 4KB pages, up to 4GB
- [x] `PageAllocator` HAL trait in `arch::mm`
- [x] AArch64 MMU setup: 4KB granule, 2MB block descriptors, 48-bit VA
- [x] Identity mapping with W^X: code RO+X (RoCode), data RW+NX (Ram), MMIO RW+NX (Device)
- [x] MAIR (3 indices), TCR (40-bit IPS, EPD1), SCTLR (MMU + D-cache + I-cache)
- [x] Linker script `__data_start` symbol at 2MB boundary for clean W^X permission split
- [x] Linked-list heap allocator with `kmalloc`/`kfree`, seeded from PMM pages (256KB)
- [x] BSP memory region constants (RAM defaults, peripheral MMIO, RP1 window)
- [x] Shell `mem` command: page stats, heap stats, MMU status
- [x] Verified on QEMU: 262K pages, MMU+caches on, timer accuracy maintained

### Phase 4 — Multitasking & Context Switch ✅

- [x] Task Control Block (TCB) with saved context, priority, state, stack, delay timer
- [x] Five task states: Ready, Running, Blocked, Suspended, Dormant
- [x] AArch64 context switch: save/restore callee-saved registers (x19-x30), task trampoline with IRQ enable
- [x] `Context` HAL trait in `arch::context`
- [x] 256-level fixed-priority scheduler with O(1) dispatch (4×u64 bitmap + trailing_zeros)
- [x] Round-robin among equal-priority tasks via per-level FIFO linked-list queues
- [x] Preemption from timer tick ISR (`sched::tick()` called from IRQ handler)
- [x] `task_create`, `task_delete`, `task_suspend`, `task_resume`, `task_yield`, `delay` API
- [x] Critical sections: DAIF masking with RAII guard (CriticalSection)
- [x] Idle task at priority 255 (WFE loop)
- [x] Shell `tasks` command showing task list with ID, name, priority, state
- [x] Verified on QEMU: multi-task context switching, timer accuracy 249/250 ticks

### Phase 5 — Synchronization Primitives ✅

- [x] Mutex with Priority Inheritance Protocol (PIP) and Priority Ceiling Protocol (PCP)
- [x] Recursive mutex locking (max nest depth 8)
- [x] Binary and counting semaphores with configurable max count
- [x] 32-bit event flags with Any/All wait modes, up to 16 concurrent waiters
- [x] Const-generic message queue `MsgQueue<MSG_SIZE, CAPACITY>` with circular buffer
- [x] Timeout support on all blocking operations (lock_timeout, wait_timeout, send_timeout, recv_timeout)
- [x] Non-blocking try variants (try_lock, try_wait, try_send, try_recv)
- [x] WaitQueue: priority-ordered waiter array with lazy stale-entry cleanup
- [x] Scheduler extended with WaitResult enum, base_priority tracking, public sync APIs
- [x] Demo tasks: mutex-protected shared counter + binary semaphore signaling between tasks
- [x] Verified on QEMU: PIP mutex, semaphore sync, correct counter handoff across tasks

### Phase 6 — Driver Framework & Logging ✅

- [x] `klog` subsystem: 5 log levels (ERROR, WARN, INFO, DEBUG, TRACE), timestamps, module tags
- [x] 64-entry ring-buffer log drain, accessible via shell `log` command
- [x] Runtime log level filter (`log level <level>` shell command)
- [x] Error/Warn messages auto-print to UART; Info/Debug/Trace to ring buffer only
- [x] Per-task execution-time budget enforcement (`task_set_budget`, `task_get_remaining`, `task_reset_budget`)
- [x] Deadline-miss detection: klog warning when task budget exhausted
- [x] Task criticality levels: SafetyCritical, MissionCritical, Standard, BestEffort
- [x] Stack watermark tracking: canary fill (0xAA) at task creation, runtime measurement
- [x] Health monitoring task (priority 1): periodic stack/CPU/watchdog checks every 5 seconds
- [x] Software watchdog: tick-based counter with auto-kick task at priority 0
- [x] CPU utilization tracking: `sched::utilization()` returns busy/total ticks
- [x] Driver trait definition with probe/remove lifecycle and static registry (16 slots)
- [x] Shell commands: `log [N]`, `log level <level>`, `health` (stack watermarks, CPU, watchdog)
- [x] Extended `tasks` command showing criticality and CPU ticks per task
- [x] Verified on QEMU: 6 tasks (incl. watchdog-kick and health-mon), no timeouts, stable operation

### Phase 7 — Symmetric Multiprocessing (SMP) ✅

- [x] Spin-table secondary core wakeup in boot.S (release addresses in `.data` section)
- [x] Secondary boot path: EL3/EL2→EL1 drop, per-core stack, FP/SIMD, vectors, MMU, GIC CPU interface, timer
- [x] Per-core stacks (8KB each for secondary cores, 512KB for primary)
- [x] `SmpBoot` HAL trait (`arch::smp`) with `core_id()`, `num_cores()`, `start_core()`
- [x] AArch64 SMP implementation using spin-table + SEV wakeup
- [x] Ticket spinlock (`SpinLock`) with IRQ save/restore for SMP mutual exclusion
- [x] Global run queue with spinlock (shared 256-level ready queues across all cores)
- [x] Per-core current task tracking (`current: [u8; MAX_CORES]`) and per-core idle tasks
- [x] IPI via GIC SGI #0 for cross-core reschedule when idle cores should pick up work
- [x] Scheduler lock held across context_switch, released by resumed task (or task_trampoline for new tasks)
- [x] UART print serialization via spinlock (clean output across concurrent cores)
- [x] Shell `smp` command and `info` command showing core count and current core
- [x] Verified on QEMU: all 4 cores online, tasks migrating across cores 0-3, mutex/semaphore correct across cores

### Phase 8 — Storage & DMA ✅

- [x] `DmaEngine` HAL trait (`arch::dma`) with channel-based configure/start/complete/abort API
- [x] `BlockDevice` HAL trait (`arch::block`) with read_block/write_block/block_count/block_size
- [x] Zero-copy buffer pool (`kernel::netbuf`): 1024 × 1536B buffers in 2MB non-cacheable region at 0x0100_0000
- [x] `MemKind::NonCacheable` MMU support using MAIR index 2 (Normal NC)
- [x] Contiguous page allocator (`alloc_pages`) for DMA pool allocation
- [x] SDHCI/EMMC2 SD card driver (`arch::aarch64::emmc2`): full card init (CMD0/8/ACMD41/2/3/9/7), PIO read/write (CMD17/CMD24), CSD v1/v2 parsing
- [x] MBR partition table parser (`kernel::storage::mbr`): 4 entries, type identification, signature validation
- [x] LRU write-back block cache (`kernel::storage::cache`): 32 lines, dirty tracking, eviction with write-back
- [x] IRQ dispatch table enlarged from 64 to 256 entries for EMMC2 INTID support
- [x] Shell commands: `sd` (card info, partitions), `sdread <lba>` (sector hex dump), `mem` (netbuf stats)
- [x] Verified on QEMU: boots with graceful SD card detection (QEMU uses SDHOST, not SDHCI)
- [x] Both BSPs (QEMU and RPi5) build cleanly

### Phase 9 — Filesystem & Shell ✅

- [x] Storage layer routing (`kernel::storage::mod`) between EMMC2 and RamDisk with static BlockCache
- [x] RamDisk BlockDevice (`kernel::storage::ramdisk`): 256 KB `.bss` array formatted as FAT32 at init
- [x] VFS layer (`kernel::fs::mod`): 16-entry fd table, open/read/write/close/readdir_open/readdir_next API
- [x] FAT32 driver (`kernel::fs::fat32`): BPB parsing, FAT chain traversal, directory parsing with LFN support
- [x] Path resolution: case-insensitive name matching, nested directory support
- [x] File read: sequential read with cluster chain caching, sector-by-sector through block cache
- [x] File write: cluster chain extension via alloc_cluster, read-modify-write for partial sectors
- [x] File create: 8.3 short name generation, free directory entry scan, cluster allocation
- [x] Directory entry size update on close for writable files
- [x] PL011 UART FIFO enabled for reliable serial RX
- [x] Shell FIFO drain: burst-read all available UART bytes before yielding
- [x] Shell commands: `ls [path]`, `cat <path>`, `hexdump <path>`, `touch <path>`, `write <path> <text>`
- [x] SMP timeout: hardware counter-based 3-second deadline for secondary core wakeup
- [x] Verified on QEMU: ramdisk mount, ls/cat/hexdump/touch/write all functional, all 4 cores online
- [x] Both BSPs (QEMU and RPi5) build cleanly

### Phase 10 — Networking & User Mode ✅

- [x] `NetDevice` HAL trait (`arch::net`) with send/recv/mac_addr for zero-copy packet I/O
- [x] `UserContext` HAL trait (`arch::user`) for EL0 task creation
- [x] Network stack: Ethernet frame parse/build, ARP cache (16 entries), IPv4 with checksum, ICMP echo, UDP, minimal TCP (client SYN/ACK/FIN)
- [x] BSD socket API: socket/bind/connect/sendto/recvfrom/close (8-entry socket table)
- [x] Loopback NetDevice for QEMU testing (swaps src/dst, converts ICMP request→reply)
- [x] Network task polling loop with loopback device init (IP 127.0.0.1)
- [x] Per-task TTBR0 page tables: clone kernel L0/L1/L2, L3 4KB pages for user stacks with guard page
- [x] ASID-tagged address spaces (8-bit ASID per user task, TLBI on switch)
- [x] `task_trampoline_user` in assembly: releases sched lock, sets SPSR_EL1=0 (EL0t), erets to user entry
- [x] `task_create_user` API: kernel stack + user stack + TTBR0 allocation
- [x] TTBR0 swap in scheduler on context switch between tasks with different page tables
- [x] Syscall dispatch via SVC #0: X8=syscall number, X0-X1=args, X0=return value
- [x] Six syscalls: SYS_YIELD(0), SYS_DELAY(1), SYS_WRITE(2), SYS_TASK_ID(3), SYS_UPTIME(4), SYS_EXIT(5)
- [x] `.user.text` linker section at 0x200000 (2MB-aligned) with EL0-accessible permissions
- [x] User demo task: prints via sys_write, delays via sys_delay, runs indefinitely at EL0
- [x] EL0 fault handling: data/prefetch abort from EL0 logs registers and terminates task
- [x] DISCARD_SP pattern for task_terminate context switch (avoids cascading faults)
- [x] Shell commands: `ping <ip>`, `netstat` (ARP/socket/config), `ifconfig` (IP/MAC/link)
- [x] Verified on QEMU: loopback ping, user task at EL0, syscalls, no faults, stable operation
- [x] Both BSPs (QEMU and RPi5) build cleanly

## Phase 11 Deliverables Checklist (next)

- [ ] `os_cfg` module with compile-time validation
- [ ] Safety-critical mode (pool-only allocation, mandatory budgets/watchdog)
- [ ] Requirements traceability matrix
- [ ] MC/DC coverage instrumentation
- [ ] WCET measurement harness