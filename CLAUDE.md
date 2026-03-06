# tiny_os — Project Context for Claude Code

## What This Is

tiny_os is a bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). It is designed for portability to other ARM cores (Cortex-A and Cortex-M families). The full specifications and implementation phases are in `docs/`.

## Current Phase

**Phase 1: Bare-Metal Bootstrap & UART Console** — get the toolchain working, boot AArch64 in EL1, print to the serial console via RP1 UART.

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
- **Timer:** ARM Generic Timer (CNTP_CTL_EL0 / CNTP_TVAL_EL0), frequency from CNTFRQ_EL0
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
| `PageAllocator`      | `mm::alloc`      | 3     | Physical page frame management        |
| `AddressSpace`       | `arch::mmu`      | 3     | Virtual memory / translation tables   |
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
│   │   ├── sched/          # Scheduler, task model (Phase 4+)
│   │   ├── sync/           # Mutex, semaphore, MQ (Phase 5+)
│   │   ├── mm/             # Page allocator, heap, VMM (Phase 3+)
│   │   ├── fs/             # VFS, FAT32 (Phase 9+)
│   │   ├── net/            # TCP/IP, sockets (Phase 10+)
│   │   ├── drivers/        # Driver traits + registry (Phase 6+)
│   │   ├── shell/          # Interactive shell (Phase 9+)
│   │   └── klog/           # Kernel logging (Phase 6+)
│   └── link.ld             # Linker script
├── arch/                   # Architecture-specific crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── aarch64/
│           ├── boot.S      # _start entry, secondary core parking
│           ├── vectors.S   # Exception vector table (Phase 2+)
│           ├── context.rs  # Context switch (Phase 4+)
│           ├── mmu.rs      # Translation tables (Phase 3+)
│           ├── gic.rs      # GIC-400 driver (Phase 2+)
│           ├── timer.rs    # Generic Timer (Phase 2+)
│           └── smp.rs      # Multi-core startup (Phase 7+)
└── bsp/                    # Board Support Packages
    ├── Cargo.toml
    └── src/
        ├── rpi5/
        │   ├── mod.rs
        │   ├── rp1_uart.rs
        │   ├── rp1_gpio.rs     # Phase 6+
        │   ├── rp1_eth.rs      # Phase 10+
        │   ├── emmc2.rs        # Phase 8+
        │   └── memory_map.rs
        └── qemu_virt/          # QEMU virt/raspi4b target
            └── mod.rs
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

## Phase 1 Deliverables Checklist

- [ ] Cargo workspace with `kernel`, `arch`, `bsp` crates
- [ ] Linker script: kernel at 0x80000, sections: .text, .rodata, .data, .bss, .stack
- [ ] `_start` in AArch64 assembly: park secondary cores (WFE), zero .bss, set SP, branch to kmain
- [ ] RP1 UART driver (MMIO volatile writes to RP1 peripheral window)
- [ ] `kprint!()` / `kprintln!()` macros via `core::fmt::Write`
- [ ] `panic_handler` that prints message + location, then infinite WFE
- [ ] Build pipeline: `cargo build` → `cargo objcopy` → `kernel8.img`
- [ ] Boot test on QEMU (`-M raspi4b -serial stdio`) and/or real Pi 5 hardware