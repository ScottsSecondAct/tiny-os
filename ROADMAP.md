# Roadmap

tiny_os is developed in 11 progressive phases. Each phase builds on the previous and has a defined set of deliverables. Phases marked ✅ are complete.

---

## Phase 1 — Bare-Metal Bootstrap & UART Console ✅

Get the toolchain working, boot AArch64 in EL1, and print to the serial console.

**Deliverables:**
- [x] Cargo workspace with `kernel`, `arch`, `bsp` crates
- [x] Linker script: kernel at `0x80000`, sections `.text`, `.rodata`, `.data`, `.bss`, `.stack`
- [x] `_start` in AArch64 assembly: park secondary cores (WFE), zero `.bss`, set SP, branch to `kmain`
- [x] RP1 UART driver (MMIO volatile writes to RP1 peripheral window at `0x1F_0006_C000`)
- [x] `kprint!()` / `kprintln!()` macros via `core::fmt::Write`
- [x] `panic_handler` that prints message + location, then infinite WFE
- [x] Build pipeline: `cargo build` → `cargo objcopy` → `kernel8.img`
- [x] QEMU test target (`-M raspi4b -serial stdio`) with PL011 UART

---

## Phase 2 — Interrupts & Timer ✅

Wire up the GIC-400 interrupt controller and the ARM Generic Timer to produce a periodic system tick.

**Deliverables:**
- [x] Exception vector table (`vectors.S`) — 2KB aligned, 16 entries, full TrapFrame save/restore
- [x] GIC-400 driver: distributor + CPU interface init, IRQ enable/disable/priority, EOI
- [x] `InterruptController` HAL trait
- [x] ARM Generic Timer driver: virtual timer (CNTV_*_EL0), CVAL-based acknowledge, 1 kHz tick
- [x] `Timer` HAL trait with periodic tick and monotonic clock
- [x] System tick ISR incrementing a global tick counter (AtomicU64)
- [x] Sync exception handler: SVC detection (advance ELR), ESR decoding
- [x] Interactive shell: help, uptime, ticks, info, svc, reboot
- [x] Boot: EL3→EL1 (secure) on QEMU raspi4b, EL2→EL1 on real Pi 5
- [x] Verified: 250 ticks in 250 ms (±0.4% accuracy)

---

## Phase 3 — Memory Management ✅

Physical memory discovery from the device tree, page frame allocator, MMU identity mapping, and heap allocator.

**Deliverables:**
- [x] Minimal FDT (DTB) parser for /memory node RAM discovery (falls back to BSP defaults)
- [x] Boot.S preserves firmware DTB pointer (x19 → DTB_PTR global)
- [x] Bitmap page frame allocator: 1 bit per 4KB page, up to 4GB (1M pages)
- [x] `PageAllocator` HAL trait in `arch::mm`
- [x] AArch64 MMU: 4KB granule, 2MB block descriptors, 48-bit VA, identity mapping
- [x] MAIR (Device-nGnRnE / Normal WB / Normal NC), TCR (40-bit IPS, EPD1), SCTLR (MMU + caches)
- [x] W^X memory policy: code mapped RO+X (RoCode), data/heap/stack mapped RW+NX, MMIO mapped RW+NX
- [x] Linker script 2MB-aligned `__data_start` boundary for clean W^X permission split
- [x] BSP memory region constants for both QEMU (1GB RAM, peripherals) and Pi 5 (4GB RAM, peripherals + RP1)
- [x] Linked-list heap allocator: `kmalloc`/`kfree`, seeded with 64 PMM pages (256KB)
- [x] Shell `mem` command: page stats (total/used/free), heap stats, MMU on/off
- [x] Verified on QEMU: 262K pages, MMU+caches on, timer accuracy maintained (252 ticks/250ms)

---

## Phase 4 — Multitasking & Context Switch ✅

Preemptive multitasking with a 256-level fixed-priority scheduler.

**Deliverables:**
- [x] Task Control Block (TCB) with saved SP, ID, priority, state, timeslice, delay timer, name, linked-list pointer
- [x] Five task states: Ready, Running, Blocked, Suspended, Dormant
- [x] AArch64 context switch: save/restore callee-saved registers (x19-x30), task trampoline with IRQ enable
- [x] `Context` HAL trait in `arch::context` with `new_context` and `switch`
- [x] 256-level fixed-priority scheduler with O(1) dispatch (4×u64 bitmap + trailing_zeros)
- [x] Round-robin among equal-priority tasks via per-priority FIFO linked-list queues
- [x] Preemption from timer tick ISR (tick decrements delay timers, wakes blocked tasks, checks timeslice)
- [x] `task_create`, `task_delete`, `task_suspend`, `task_resume`, `task_yield`, `delay` API
- [x] Critical sections: DAIF masking with RAII guard (CriticalSection)
- [x] Idle task at priority 255 (WFE loop, auto-created at init)
- [x] Shell `tasks` command (task list with ID, name, priority, state) and `yield` command
- [x] Verified on QEMU: multi-task preemptive scheduling, timer accuracy 249/250 ticks

---

## Phase 5 — Synchronization Primitives ✅

Blocking synchronization objects with priority inversion protection.

**Deliverables:**
- [x] Mutex with Priority Inheritance Protocol (PIP) and Priority Ceiling Protocol (PCP)
- [x] Recursive mutex locking with configurable max nest depth (8)
- [x] Binary and counting semaphores with configurable max count
- [x] 32-bit event flags with Any/All wait modes, up to 16 concurrent waiters
- [x] Const-generic message queue `MsgQueue<MSG_SIZE, CAPACITY>` with circular buffer, separate send/recv wait queues
- [x] Timeout support on all blocking operations (integrates with Phase 2 timer via scheduler delay)
- [x] Non-blocking try variants on all primitives (try_lock, try_wait, try_send, try_recv)
- [x] WaitQueue: priority-ordered waiter management with lazy stale-entry cleanup
- [x] Scheduler extensions: WaitResult enum, base_priority tracking, public APIs for sync primitives (block/wake/set_priority)
- [x] Demo: mutex-protected shared counter with binary semaphore signaling between tasks
- [x] Verified on QEMU: PIP mutex, semaphore task synchronization, correct counter handoff

---

## Phase 6 — Driver Framework & Logging ✅

A structured driver registry, kernel logging subsystem, health monitoring, and budget enforcement.

**Deliverables:**
- [x] `klog` subsystem: 5 log levels (ERROR, WARN, INFO, DEBUG, TRACE), timestamps, module tags
- [x] 64-entry ring-buffer log drain, accessible via shell `log` command with runtime level filter
- [x] Error/Warn auto-print to UART; Info/Debug/Trace to ring buffer only
- [x] Driver trait definition with probe/remove lifecycle and 16-slot static registry
- [x] Per-task execution-time budget enforcement (`task_set_budget`, `task_get_remaining`, `task_reset_budget`)
- [x] Deadline-miss detection via klog warning when task budget exhausted
- [x] Task criticality levels: SafetyCritical, MissionCritical, Standard, BestEffort
- [x] Stack watermark tracking: 0xAA canary fill at task creation, runtime scan for high-water mark
- [x] Health monitoring task (priority 1): periodic stack/CPU/watchdog checks every 5 seconds
- [x] Software watchdog: tick-based counter, `watchdog_init`/`watchdog_kick`, auto-kick task at priority 0
- [x] CPU utilization tracking: `sched::utilization()` returns busy/total ticks
- [x] Shell commands: `log [N]`, `log level <level>`, `health` (stack watermarks, CPU, watchdog status)
- [x] Extended `tasks` command: criticality column and per-task CPU tick counter
- [x] Verified on QEMU: 6 tasks (incl. watchdog-kick and health-mon), stable operation, no timeouts

---

## Phase 7 — Symmetric Multiprocessing (SMP) ✅

Bring up all four Cortex-A76 cores and extend the scheduler for multi-core operation.

**Deliverables:**
- [x] Secondary core wakeup via spin-table in boot.S (per-core release addresses in `.data`)
- [x] Secondary core boot path: EL3/EL2→EL1 drop, per-core stack, FP/SIMD, vectors, MMU, GIC, timer
- [x] Per-core stacks (8KB each) and GIC CPU interface initialization per secondary core
- [x] `SmpBoot` HAL trait (`arch::smp`) with `core_id()`, `num_cores()`, `start_core()`
- [x] AArch64 SMP implementation (`arch::aarch64::smp`) using spin-table wakeup + SEV
- [x] Ticket spinlock (`SpinLock`) for SMP mutual exclusion with IRQ save/restore
- [x] Global run queue with spinlock (all cores share one set of 256-level ready queues)
- [x] Per-core current task tracking and per-core idle tasks
- [x] IPI via GIC SGI #0 for cross-core reschedule notifications
- [x] UART print serialization via spinlock (no interleaved output across cores)
- [x] Scheduler lock protocol: held across context_switch, released by resumed task (or task_trampoline for new tasks)
- [x] Verified on QEMU: all 4 cores online, tasks migrating across cores, correct mutex/semaphore operation

---

## Phase 8 — Storage & DMA ✅

SD card access via BCM2712 EMMC2 (SDHCI), a DMA engine HAL, zero-copy buffer pool, and block-level storage abstractions.

**Deliverables:**
- [x] `DmaEngine` HAL trait (`arch::dma`): channel-based configure, start, complete, abort API
- [x] `BlockDevice` HAL trait (`arch::block`): sector read/write with caller-provided buffers
- [x] Zero-copy buffer pool (`kernel::netbuf`): 1024 × 1536B buffers in 2MB non-cacheable region (MAIR index 2), AtomicU8 refcount, spinlock-protected free list
- [x] `MemKind::NonCacheable` MMU block descriptor support
- [x] Contiguous page allocator (`alloc_pages`) for DMA pool allocation
- [x] SDHCI/EMMC2 SD card driver (`arch::aarch64::emmc2`): CMD0/8/ACMD41/2/3/9/7 card init, PIO read (CMD17) and write (CMD24), CSD v1/v2 card size parsing
- [x] MBR partition table parser: 4 entries, type identification (FAT12/16/32, NTFS, Linux, etc.), 0xAA55 signature validation
- [x] LRU write-back block cache: 32 lines × 512B, dirty tracking, tick-based LRU eviction
- [x] IRQ dispatch table enlarged to 256 entries for EMMC2 INTID support
- [x] Shell commands: `sd` (card info, partitions), `sdread <lba>` (sector hex dump)
- [x] Verified on QEMU: graceful fallback when SDHCI not present (QEMU raspi4b uses SDHOST)
- [x] Both BSPs (QEMU and RPi5) build cleanly

---

## Phase 9 — Filesystem & Shell

FAT32 filesystem and an interactive kernel shell over UART.

**Deliverables:**
- [ ] VFS abstraction layer
- [ ] FAT32 driver: read/write files and directories, long filename support
- [ ] `open`, `read`, `write`, `close`, `readdir` VFS API
- [ ] Interactive UART shell (`shell/`) with command dispatch
- [ ] Built-in shell commands: `ls`, `cat`, `hexdump`, `tasks`, `log`, `mem`
- [ ] Kernel module loading from FAT32 (stretch goal)

---

## Phase 10 — Networking & User Mode

Gigabit Ethernet via RP1, a zero-copy TCP/IP stack, BSD-style sockets, and EL0 user tasks. The entire RX and TX data path uses the `NetBuf` pool from Phase 8 — no buffer copies between layers.

**Deliverables:**
- [ ] RP1 Gigabit Ethernet driver (`rp1_eth.rs`): TX/RX descriptor rings pointing to `NetBuf` buffers, MDIO PHY management
- [ ] `NetDevice` HAL trait: zero-copy packet TX/RX (accepts/returns `NetBuf` references, not byte slices)
- [ ] Zero-copy RX path: NIC DMA → `NetBuf` → IP/TCP header parse in-place → socket `recv()` returns buffer reference
- [ ] Zero-copy TX path: app writes to `NetBuf` → TCP/IP prepend headers via `head` adjust → DMA descriptor → NIC transmit → buffer reclaim
- [ ] ARP, IPv4, ICMP (ping), UDP, TCP (basic state machine) — all operating on `NetBuf` without intermediate copies
- [ ] BSD socket API: `socket`, `bind`, `connect`, `send`, `recv`, `close`
- [ ] EL0 user task support: `UserContext` HAL trait, EL1→EL0 drop, syscall table
- [ ] Memory isolation: per-task address space (extends Phase 3 VMM)
- [ ] `ping` and `httpget` demo tasks

---

## Phase 11 — Safety Certification

Produce the evidence and tooling required for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C certification.

**Deliverables:**
- [ ] `os_cfg` module with compile-time validation (`const` assertions for all `OS_CFG_*` constants)
- [ ] Safety-critical mode (`OS_CFG_SAFETY_CRITICAL`): pool-only allocation, mandatory budgets/watchdog/health monitor
- [ ] Requirements traceability matrix (`docs/traceability.csv`) with bidirectional coverage
- [ ] MC/DC coverage instrumentation via LLVM `-C instrument-coverage`, per-component targets (100% scheduler/sync, 90%+ drivers)
- [ ] WCET measurement harness: cycle-counter instrumentation for all kernel services (spec section 4.6)
- [ ] Schedulability analysis tool: RMA utilization check + response-time analysis with PIP blocking
- [ ] Fault injection test suite: stack overflow, invalid memory, budget exhaustion, watchdog timeout
- [ ] Certification evidence package: coverage reports, WCET reports, traceability matrix, shutdown logs
- [ ] Ferrocene qualified toolchain integration and build-system support

---

## Long-Term / Stretch Goals

- POSIX-compatible process model and `fork`/`exec`
- USB host via RP1 (keyboard/storage)
- Rust `#[async_fn]` cooperative tasks alongside preemptive tasks
- Port to Cortex-M targets (RP2040, STM32)
- Automated hardware-in-the-loop CI on real Pi 5
