# tiny_os — Project Context for Claude Code

## What This Is

tiny_os is a bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). It is designed for portability to other ARM cores (Cortex-A and Cortex-M families). The specification (v1.2) includes RTOS certification provisions for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C — covering WCET bounds, MC/DC coverage, health monitoring, watchdog integration, structured shutdown, mixed-criticality partitioning, and requirements traceability. The full specifications and implementation phases are in `docs/`.

## Current Phase

**Phase 7: Symmetric Multiprocessing (SMP)** — complete. All 4 cores online via spin-table wakeup in boot.S, ticket spinlock for SMP mutual exclusion, global run queue with spinlock protection, per-core current task tracking, per-core idle tasks, IPI via GIC SGI for cross-core reschedule, serialized UART output. Tasks migrate across all 4 cores. Built on Phase 6's logging/watchdog/health and Phase 5's sync primitives. Next up: Phase 8 (Storage & DMA).

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
│   │   ├── shell.rs        # Interactive UART shell (help, uptime, ticks, info, mem, tasks, log, health, smp, yield, svc, reboot)
│   │   ├── sched.rs        # SMP-aware 256-level fixed-priority scheduler: per-core current, spinlock, IPI
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
│   │       ├── mod.rs      # MM init: RAM discovery, PMM, MMU enable, heap seeding
│   │       ├── dtb.rs      # Minimal FDT parser for /memory node
│   │       ├── pmm.rs      # Bitmap page frame allocator (4KB pages, up to 4GB)
│   │       └── heap.rs     # Linked-list heap allocator (kmalloc/kfree)
│   └── link.ld             # Linker script
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
│       └── aarch64/
│           ├── mod.rs
│           ├── boot.S      # _start, spin-table secondary parking, secondary_boot EL drop
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries)
│           ├── exceptions.rs # TrapFrame, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 driver: distributor, CPU interface, SGI for IPI
│           ├── timer.rs    # ARM Generic Timer (virtual timer, 1kHz tick, secondary init)
│           ├── mmu.rs      # MMU: identity mapping, 2MB blocks, W^X, secondary core init
│           ├── smp.rs      # AArch64 SMP: spin-table wakeup, core_id, start_core
│           ├── context.rs  # Aarch64Context: new_context (fake frame), switch wrapper
│           └── context_switch.S  # Context switch (x19-x30) + task trampoline (sched lock release)
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

## Phase 8 Deliverables Checklist (next)

- [ ] DMA engine driver (`DmaEngine` HAL trait)
- [ ] Zero-copy buffer pool (`NetBuf`)
- [ ] EMMC2 / SDIO driver
- [ ] `BlockDevice` HAL trait
- [ ] Partition table parsing (MBR)
- [ ] Block cache (LRU, write-back)