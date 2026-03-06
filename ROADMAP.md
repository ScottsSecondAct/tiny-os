# Roadmap

tiny_os is developed in 10 progressive phases. Each phase builds on the previous and has a defined set of deliverables. Phases marked ✅ are complete.

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
- [x] QEMU test target (`-M raspi3b -serial stdio`) with PL011 UART

---

## Phase 2 — Interrupts & Timer

Wire up the GIC-400 interrupt controller and the ARM Generic Timer to produce a periodic system tick.

**Deliverables:**
- [ ] Exception vector table (`vectors.S`) for EL1
- [ ] GIC-400 driver: distributor + CPU interface init, IRQ enable/disable, EOI
- [ ] `InterruptController` HAL trait
- [ ] ARM Generic Timer driver: CNTP_CTL_EL0 / CNTP_TVAL_EL0, frequency from CNTFRQ_EL0
- [ ] `Timer` HAL trait with periodic tick and monotonic clock
- [ ] System tick ISR incrementing a global tick counter
- [ ] `kprintln!` from interrupt context (DAIF-safe)

---

## Phase 3 — Memory Management

Physical memory discovery from the device tree, page frame allocator, and virtual memory via AArch64 translation tables.

**Deliverables:**
- [ ] Device tree (DTB) parser for memory regions and peripheral addresses
- [ ] Physical page frame allocator (`PageAllocator` trait) — bitmap or buddy system
- [ ] AArch64 translation table setup (4 KB pages, 48-bit VA)
- [ ] `AddressSpace` HAL trait
- [ ] Identity mapping for kernel, MMIO device regions mapped as device memory
- [ ] `kmalloc` / `kfree` heap allocator (slab or linked-list)
- [ ] Memory stats via `kprintln!`

---

## Phase 4 — Multitasking & Context Switch

Preemptive multitasking with a 256-level fixed-priority scheduler.

**Deliverables:**
- [ ] Task Control Block (TCB) with 20 fields (ID, priority, state, saved context, stack, timing)
- [ ] Five task states: Ready, Running, Blocked, Suspended, Dormant
- [ ] AArch64 context switch: save/restore all general-purpose + FP/SIMD registers
- [ ] `Context` HAL trait
- [ ] 256-level fixed-priority scheduler with O(1) dispatch (bitmap + CLZ)
- [ ] Round-robin among equal-priority tasks via per-level FIFO queues
- [ ] Preemption from timer tick ISR
- [ ] `task_create`, `task_delete`, `task_suspend`, `task_resume` API
- [ ] Critical sections: DAIF masking with nesting count

---

## Phase 5 — Synchronization Primitives

Blocking synchronization objects with priority inversion protection.

**Deliverables:**
- [ ] Mutex with Priority Inheritance Protocol (PIP) and Priority Ceiling Protocol (PCP)
- [ ] Binary and counting semaphores
- [ ] Message queue (fixed-size, bounded)
- [ ] Event flags
- [ ] Timeout support on all blocking operations (integrates with Phase 2 timer)
- [ ] Deadlock detection (debug builds)

---

## Phase 6 — Driver Framework & Logging

A structured driver registry and a kernel logging subsystem to replace early `kprintln!` boot prints.

**Deliverables:**
- [ ] Driver trait registry with probe/remove lifecycle
- [ ] GPIO driver via RP1 (`rp1_gpio.rs`)
- [ ] SPI and I²C drivers via RP1 (basic)
- [ ] `klog` subsystem: log levels (ERROR, WARN, INFO, DEBUG, TRACE), timestamps, module tags
- [ ] Ring-buffer log drain (accessible via shell in Phase 9)

---

## Phase 7 — Symmetric Multiprocessing (SMP)

Bring up all four Cortex-A76 cores and extend the scheduler for multi-core operation.

**Deliverables:**
- [ ] Secondary core wakeup sequence (`smp.rs`) via spin-table / PSCI
- [ ] Per-core stacks and GIC CPU interface initialization
- [ ] `SmpBoot` HAL trait
- [ ] Per-core run queues with work-stealing or global run queue with spinlock
- [ ] Spinlock (`SpinMutex`) for SMP critical sections
- [ ] IPI (inter-processor interrupts) for scheduler cross-core wakeup
- [ ] Verified preemption and context switch on all 4 cores simultaneously

---

## Phase 8 — Storage

SD card access via BCM2712 EMMC2 and a lightweight block layer.

**Deliverables:**
- [ ] EMMC2 / SDIO driver (`emmc2.rs`): CMD0/2/3/7/8/9/17/18/24/25 support
- [ ] `BlockDevice` HAL trait: sector read/write
- [ ] DMA engine driver (`DmaEngine` trait) for zero-copy block transfers
- [ ] Partition table parsing (MBR)
- [ ] Block cache (simple LRU, write-back)

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

Gigabit Ethernet via RP1, a TCP/IP stack, BSD-style sockets, and EL0 user tasks.

**Deliverables:**
- [ ] RP1 Gigabit Ethernet driver (`rp1_eth.rs`)
- [ ] `NetDevice` HAL trait: packet TX/RX
- [ ] ARP, IPv4, ICMP (ping), UDP, TCP (basic state machine)
- [ ] BSD socket API: `socket`, `bind`, `connect`, `send`, `recv`, `close`
- [ ] EL0 user task support: `UserContext` HAL trait, EL1→EL0 drop, syscall table
- [ ] Memory isolation: per-task address space (extends Phase 3 VMM)
- [ ] `ping` and `httpget` demo tasks

---

## Long-Term / Stretch Goals

- POSIX-compatible process model and `fork`/`exec`
- USB host via RP1 (keyboard/storage)
- Rust `#[async_fn]` cooperative tasks alongside preemptive tasks
- Port to Cortex-M targets (RP2040, STM32)
- Automated hardware-in-the-loop CI on real Pi 5
