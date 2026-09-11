# Roadmap

tiny_os is developed in 12 progressive phases. Each phase builds on the previous and has a defined set of deliverables. Phases marked ✅ are complete.

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

## Phase 9 — Filesystem & Shell ✅

FAT32 filesystem with VFS abstraction, RamDisk for QEMU testing, and extended shell.

**Deliverables:**
- [x] Storage layer routing between EMMC2 and RamDisk with static BlockCache
- [x] RamDisk BlockDevice: 256 KB in-memory FAT32 volume for QEMU testing
- [x] VFS abstraction layer: 16-entry fd table, FsError enum, DirEntry type
- [x] FAT32 driver: BPB parsing, FAT chain traversal, directory parsing with LFN support
- [x] `open`, `read`, `write`, `close`, `readdir_open`, `readdir_next` VFS API
- [x] Path resolution: case-insensitive name matching, nested directory support
- [x] File read/write with cluster chain caching, file create with 8.3 short name generation
- [x] PL011 UART FIFO enabled for reliable serial RX; shell FIFO drain
- [x] Shell commands: `ls [path]`, `cat <path>`, `hexdump <path>`, `touch <path>`, `write <path> <text>`
- [x] SMP timeout: hardware counter-based 3-second deadline for secondary core wakeup
- [x] Verified on QEMU: ramdisk mount, all fs commands functional, all 4 cores online
- [x] Both BSPs (QEMU and RPi5) build cleanly

---

## Phase 10 — Networking & User Mode ✅

Zero-copy network stack with loopback device for QEMU testing, BSD-style sockets, and EL0 user tasks with per-task page tables.

**Deliverables:**
- [x] `NetDevice` HAL trait (`arch::net`): zero-copy packet TX/RX via NetBuf indices
- [x] `UserContext` HAL trait (`arch::user`): EL0 task context creation
- [x] Loopback NetDevice for QEMU: swaps src/dst addresses, converts ICMP echo request→reply
- [x] Network stack: Ethernet frame parse/build, ARP cache (16 entries), IPv4 with internet checksum, ICMP echo, UDP port table, minimal TCP (client SYN/ACK/FIN, 4 connections)
- [x] BSD socket API: `socket`, `bind`, `connect`, `sendto`, `recvfrom`, `close` (8-entry table)
- [x] Network task: poll loop for loopback device, IP 127.0.0.1
- [x] Per-task TTBR0 page tables: clone kernel tables, L3 4KB pages for user stacks with guard page
- [x] ASID-tagged address spaces (8-bit ASID per user task), TLBI on TTBR0 switch
- [x] `task_trampoline_user`: releases sched lock, sets SPSR_EL1=0 (EL0t), erets to user entry
- [x] `task_create_user` API with kernel stack, user stack, and TTBR0 allocation
- [x] Basic syscall dispatch via SVC #0: SYS_YIELD(0), SYS_DELAY(1), SYS_WRITE(2), SYS_TASK_ID(3), SYS_UPTIME(4), SYS_EXIT(5), SYS_TEMPERATURE(6)
- [x] Subsystem multiplexed syscalls: SYS_FS(10), SYS_NET(11), SYS_SPI(12), SYS_I2C(13), SYS_GPIO(14) with X0=operation, X1-X3=args
- [x] `.user.text` linker section at 0x200000 with EL0-accessible permissions
- [x] User demo task running at EL0, printing via syscalls
- [x] EL0 fault handling: register dump + task termination, DISCARD_SP pattern for safe context switch
- [x] VideoCore mailbox driver (`arch::aarch64::mailbox`): property tag interface, SoC temperature query (tag 0x00030006)
- [x] `SpiDevice`, `I2cDevice`, `GpioController` HAL traits with RP1 southbridge driver implementations
- [x] RP1 SPI0 (DW_apb_ssi), I2C0 (DW_apb_i2c), GPIO (28-pin, pad control, RIO) — cfg-gated for bsp-rpi5
- [x] Kernel peripheral manager (`periph.rs`): static driver instances, error stubs on QEMU
- [x] User-space temperature monitor (`examples/temp_monitor/`): SoC temp via SYS_TEMPERATURE, min/max/avg stats
- [x] User-space sensor gateway (`examples/sensor_gateway/`): SPI/I2C/GPIO data collection, SD card logging, UDP telemetry
- [x] Shell commands: `ping <ip>`, `netstat`, `ifconfig`, `temp`
- [x] Verified on QEMU: loopback ping, user task at EL0, syscalls, temperature monitor, sensor gateway with graceful hw fallback, no faults, stable operation
- [x] Both BSPs (QEMU and RPi5) build cleanly, all 4 BSP×feature configurations pass

---

## Test Infrastructure ✅

Two-tier testing accommodates the bare-metal workspace constraint (`aarch64-unknown-none` default target has no `std`).

**Host-side unit tests** (`tests/host/`, 28 tests):
- [x] IPv4 checksum and header parsing (12 tests): RFC 1071 example, corruption detection, protocol parsing
- [x] Ethernet frame parsing (7 tests): EtherType demux, header validation, broadcast detection
- [x] MBR partition table parsing (7 tests): FAT32/Linux/swap types, signature validation, multi-partition
- [x] Separate `std` crate with `--target x86_64-pc-windows-msvc` override via Makefile

**QEMU integration tests** (`tests/qemu/run_tests.ps1`, 13 checks):
- [x] Boots kernel on QEMU raspi4b, captures serial output with 15s timeout
- [x] Verifies: kernel banner, UART init, MMU, timer, scheduler, SMP cores 1-3, network loopback, filesystem mount, user task at EL0, shell prompt, no kernel panic
- [x] Saves output to `tests/qemu/last_output.txt` for debugging

**Workspace config:**
- [x] `default-members` excludes `tests/host` from bare-metal builds
- [x] Makefile targets: `make test` (both), `make test-host`, `make test-qemu`

---

## Phase 11 — Safety Certification ✅

Produce the evidence and tooling required for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C certification.

**Deliverables:**
- [x] `os_cfg` module with compile-time validation (`const` assertions for all 14 `OS_CFG_*` constants)
- [x] `safety-critical` Cargo feature flag: pool-only allocation (heap disabled), mandatory budgets/watchdog/health monitor
- [x] Fixed-size memory pool allocator (`kernel::mm::pool`): O(1) `pool_alloc`/`pool_free` with spinlock, up to 16 pools
- [x] 14 hook functions (`kernel::hooks`): idle, stack_overflow, data_abort, hard_fault, assert, task_create, task_switch, budget_overrun, deadline_miss, task_terminated, watchdog_expired, health_check_failed, shutdown, safety_critical_lost
- [x] Enhanced health monitor: 7 checks (stacks, CPU, watchdog, ready queue integrity, mutex ownership, tick monotonicity, pool accounting)
- [x] Budget enforcement: task suspension on overrun, period-based replenishment, `os_hook_budget_overrun` callback
- [x] Structured shutdown (`kernel::shutdown`): interrupt mask, diagnostic register save, fault logging, hook, reboot-or-halt
- [x] Criticality mode switch (`kernel::criticality`): `os_criticality_switch`/`os_criticality_restore` to suspend lower-criticality tasks
- [x] WCET measurement harness (`kernel::wcet`): PMU cycle counter, min/max/avg tracking for kernel services
- [x] Schedulability analysis (`kernel::sched_analysis`): RMA utilization check + Response-Time Analysis with PIP blocking
- [x] Fault injection test suite (`kernel::fault_inject`): pool exhaust, double free, bad pointer, budget overrun, hook invocation, criticality switch, diagnostic region
- [x] Requirements traceability matrix (`docs/traceability.csv`): 56 requirements with bidirectional spec-to-code-to-test mapping
- [x] Shell commands: `faulttest` (run fault injection suite), `wcet` (dump WCET measurements)
- [x] All 6 BSP×feature configurations build cleanly (bsp-qemu, bsp-rpi5, ×dynamic-load, ×safety-critical)

---

## Phase 12 — Extended Peripheral Support

Complete hardware coverage for the remaining Pi 5 peripherals most relevant to RTOS and industrial applications. Adds new subsystem syscalls and HAL traits for peripherals not yet exposed to user space.

**New syscalls:**
- `SYS_UART` (15): User-space serial port access (open, read, write, configure baud/parity/flow)
- `SYS_PWM` (16): Pulse-width modulation (configure, set duty cycle, enable/disable)
- `SYS_RTC` (17): Real-time clock (get/set wall-clock time, set alarm)
- `SYS_DMA` (18): User-space DMA transfers (configure channel, start, wait, abort)
- `SYS_USB` (19): USB host operations (enumerate, bulk/interrupt transfer, HID input)
- `SYS_CRYPTO` (20): Hardware-accelerated cryptography (AES, SHA-256, HMAC)
- `SYS_POWER` (21): Power management (sleep modes, DVFS frequency scaling, voltage query)

**Deliverables:**

*RTC & Power:*
- [ ] `RtcDevice` HAL trait: get/set time, alarm, calibration
- [ ] BCM2712 RTC driver: battery-backed RTC with alarm interrupt
- [ ] Power button handler: interrupt-driven, configurable action (shutdown/suspend/ignore)
- [ ] Power management: CPU frequency scaling (DVFS via mailbox), WFI-based idle states
- [ ] SYS_RTC and SYS_POWER syscall dispatch

*PWM:*
- [ ] `PwmDevice` HAL trait: configure channel, set frequency/duty, enable/disable
- [ ] RP1 PWM driver: 2 channels on 40-pin header (GPIO 12/13 ALT0, GPIO 18/19 ALT5)
- [ ] SYS_PWM syscall dispatch
- [ ] Example: servo or LED dimming user-space app

*UART (user-facing):*
- [ ] `SerialPort` HAL trait: open, configure (baud/parity/stop/flow), read, write, close
- [ ] RP1 UART1–UART5 drivers (separate from kernel console UART0)
- [ ] SYS_UART syscall dispatch with fd-based access model

*DMA (user-facing):*
- [ ] User-space DMA syscall wrappers around existing `DmaEngine` HAL trait
- [ ] Memory-to-memory and memory-to-peripheral transfer modes
- [ ] DMA completion notification via task wakeup (not polling)
- [ ] SYS_DMA syscall dispatch

*USB Host:*
- [ ] `UsbHostController` HAL trait: enumerate, configure endpoint, transfer (bulk/interrupt/control)
- [ ] RP1 xHCI USB 3.0 driver: port power, device enumeration, bulk/interrupt transfers
- [ ] USB mass storage class driver (read/write via BlockDevice trait)
- [ ] USB HID class driver (keyboard/mouse input events)
- [ ] SYS_USB syscall dispatch

*Ethernet (real hardware):*
- [ ] RP1 Gigabit Ethernet MAC driver: DMA ring descriptors, link negotiation, PHY management
- [ ] Integrate with existing network stack (replace loopback on real hardware)
- [ ] MDIO/PHY driver for link configuration and status
- [ ] Shell `ifconfig` showing real link speed/duplex on Pi 5

*Crypto:*
- [ ] `CryptoEngine` HAL trait: AES-128/256, SHA-256, HMAC
- [ ] ARMv8 Cryptography Extensions driver: AESE/AESD/SHA256H instructions
- [ ] SYS_CRYPTO syscall dispatch
- [ ] Example: secure sensor data signing user-space app

*SDR104 high-speed SD:*
- [ ] EMMC2 driver upgrade: SDR104 mode (208 MHz), ADMA2 DMA transfers
- [ ] UHS-I voltage switching (1.8V signaling)
- [ ] Benchmark: sequential read throughput comparison (PIO vs DMA)

*Verification:*
- [ ] All new HAL traits with cfg-gated RP1 implementations (Pi 5) and error stubs (QEMU)
- [ ] Host-side unit tests for protocol parsing (USB descriptors, Ethernet frames, crypto vectors)
- [ ] QEMU integration tests for new syscall numbers (E_NOSYS on unimplemented subsystems)
- [ ] Both BSPs build cleanly across all feature flag combinations

---

## Long-Term / Stretch Goals

- VideoCore VII GPU: framebuffer, hardware-accelerated 2D, compute shaders via mailbox
- HDMI display output: dual 4Kp60, mode setting via VideoCore firmware
- MIPI CSI camera input: 4-lane MIPI receiver, frame capture, ISP pipeline
- MIPI DSI display output: 4-lane MIPI transmitter, panel initialization
- HEVC 4Kp60 hardware video decoder via VideoCore
- Wi-Fi 802.11ac: CYW43455 driver (SDIO), WPA2/WPA3, AP mode
- Bluetooth 5.0/BLE: CYW43455 HCI transport, GATT client/server
- PCIe 2.0 x1 endpoint driver: NVMe storage, custom FPGA/accelerator boards
- POSIX-compatible process model and `fork`/`exec`
- Rust `#[async_fn]` cooperative tasks alongside preemptive tasks
- Port to Cortex-M targets (RP2040, STM32)
- Automated hardware-in-the-loop CI on real Pi 5
