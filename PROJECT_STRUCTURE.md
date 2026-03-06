# Project Structure

```
tiny_os/
├── Cargo.toml              # Workspace root (members: kernel, arch, bsp)
├── Cargo.lock              # Locked dependency versions
├── rust-toolchain.toml     # Pins nightly channel + aarch64-unknown-none target
├── Makefile                # Convenience wrapper: make / make img / make qemu
├── config.txt              # Raspberry Pi 5 firmware config (bare-metal settings)
├── LICENSE                 # MIT
├── README.md
├── PROJECT_STRUCTURE.md    # This file
├── ROADMAP.md
│
├── .cargo/
│   └── config.toml         # Linker: aarch64-linux-gnu-gcc, -nostartfiles,
│                           #   -Tkernel/link.ld; default target triple
│
├── docs/                   # Specifications (kept as reference)
│   ├── tiny_os_specification_v1.1.docx
│   ├── tiny_os_implementation_phases.docx
│   └── simple_os_scheduler_spec.docx
│
├── arch/                   # Architecture crate — hardware register access & HAL traits
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Crate root; re-exports arch-specific modules
│       ├── uart.rs         # UartDriver trait definition
│       └── aarch64/
│           ├── mod.rs      # AArch64 module root
│           ├── boot.S      # _start: park secondaries, zero BSS, set SP, bl kmain
│           │               #   Also handles EL2 → EL1 drop if firmware lands in EL2
│           ├── context.rs  # (Phase 4) Task context save/restore/switch
│           ├── mmu.rs      # (Phase 3) Translation table setup
│           ├── gic.rs      # (Phase 2) GIC-400 interrupt controller driver
│           ├── timer.rs    # (Phase 2) ARM Generic Timer (CNTP_*)
│           ├── vectors.S   # (Phase 2) Exception vector table
│           └── smp.rs      # (Phase 7) Secondary core wakeup
│
├── bsp/                    # Board Support Package crate — concrete HAL implementations
│   ├── Cargo.toml          # Features: bsp-rpi5 (default), bsp-qemu (mutually exclusive)
│   └── src/
│       ├── lib.rs          # Re-exports PlatformUart based on active feature flag
│       ├── rpi5/
│       │   ├── mod.rs          # BSP root for Raspberry Pi 5
│       │   ├── memory_map.rs   # RP1_UART0_BASE = 0x1F_0006_C000 (36-bit PCIe window)
│       │   ├── rp1_uart.rs     # RP1 PL011 UART driver (MMIO volatile writes)
│       │   ├── rp1_gpio.rs     # (Phase 6) GPIO via RP1
│       │   ├── emmc2.rs        # (Phase 8) eMMC/SD card via BCM2712 EMMC2
│       │   └── rp1_eth.rs      # (Phase 10) Gigabit Ethernet via RP1
│       └── qemu_virt/
│           └── mod.rs          # BCM2837 PL011 UART at 0x3F20_1000 (raspi3b target)
│
└── kernel/                 # Kernel binary crate
    ├── Cargo.toml          # Depends on arch + bsp; propagates bsp-* feature flags
    ├── link.ld             # Linker script: .text.boot at 0x80000, then .text,
    │                       #   .rodata, .data, .bss (16-byte aligned), .stack
    └── src/
        ├── main.rs         # kmain(): init UART, init print, print banner, WFE spin
        ├── panic.rs        # #[panic_handler]: print message + location, WFE halt
        ├── print.rs        # kprint!() / kprintln!() macros via core::fmt::Write
        ├── sched/          # (Phase 4) Scheduler, TCB, task states
        ├── sync/           # (Phase 5) Mutex, semaphore, message queue
        ├── mm/             # (Phase 3) Physical page allocator, heap, VMM
        ├── fs/             # (Phase 9) VFS, FAT32 driver
        ├── net/            # (Phase 10) TCP/IP stack, sockets
        ├── drivers/        # (Phase 6) Driver trait registry
        ├── shell/          # (Phase 9) Interactive kernel shell
        └── klog/           # (Phase 6) Structured kernel logging subsystem
```

## Key Design Constraints

- **`no_std` / `no_main`** — no Rust standard library; no C runtime.
- **Unsafe discipline** — every `unsafe` block carries a `// SAFETY:` comment.
- **MMIO** — all register accesses via `core::ptr::read_volatile` / `write_volatile`,
  wrapped in typed structs. Never cast peripheral base addresses to `u32`.
- **HAL isolation** — if it touches a hardware register, it lives in `arch/` or `bsp/`,
  never in `kernel/`. Porting requires only new trait implementations, not kernel changes.
- **Kernel load address** — `0x80000` (RPi firmware convention, enforced by `link.ld`).
- **BSP feature flags** are mutually exclusive; enabling both causes a compile error
  (duplicate `PlatformUart` definition).
