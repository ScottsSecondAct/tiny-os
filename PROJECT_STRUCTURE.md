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
│   └── tiny_os_specification_v1.1.md   # System specification v1.2 — includes RTOS
│                                       #   certification: WCET, MC/DC, health monitor,
│                                       #   watchdog, mixed-criticality, traceability
│
├── arch/                   # Architecture crate — hardware register access & HAL traits
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Crate root; re-exports arch-specific modules
│       ├── uart.rs         # UartDriver trait definition
│       ├── irq.rs          # InterruptController trait definition
│       ├── timer.rs        # Timer trait definition
│       ├── mm.rs           # PageAllocator trait definition
│       └── aarch64/
│           ├── mod.rs      # AArch64 module root
│           ├── boot.S      # _start: save DTB ptr, park secondaries, EL3→EL1 (secure)
│           │               #   or EL2→EL1 drop, zero BSS, store DTB_PTR,
│           │               #   set SP, CPACR, VBAR, bl kmain
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries),
│           │               #   TrapFrame save/restore macros, handler stubs
│           ├── exceptions.rs # TrapFrame struct, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 (GICv2) driver: distributor + CPU interface
│           ├── timer.rs    # ARM Generic Timer (virtual timer CNTV, 1kHz tick,
│           │               #   CVAL-based acknowledge)
│           └── mmu.rs      # MMU setup: static L0/L1/L2 page tables, identity
│                           #   mapping with 2MB blocks, W^X policy (RoCode RX,
│                           #   Ram RW+NX, Device NX), MAIR/TCR/SCTLR config
│
├── bsp/                    # Board Support Package crate — concrete HAL implementations
│   ├── Cargo.toml          # Features: bsp-rpi5 (default), bsp-qemu (mutually exclusive)
│   └── src/
│       ├── lib.rs          # Re-exports PlatformUart, GIC_DIST_BASE, GIC_CPU_BASE
│       │                   #   based on active feature flag
│       ├── rpi5/
│       │   ├── mod.rs          # BSP root for Raspberry Pi 5
│       │   ├── memory_map.rs   # RP1_UART0_BASE = 0x1F_0006_C000 (36-bit PCIe window),
│       │   │                   #   GIC bases, RAM default (4GB), peripheral + RP1 MMIO regions
│       │   └── rp1_uart.rs     # RP1 PL011 UART driver (MMIO volatile writes)
│       └── qemu_virt/
│           ├── mod.rs          # BSP root for QEMU raspi4b
│           ├── memory_map.rs   # UART at 0xFE20_1000, GIC bases, RAM default (1GB),
│           │                   #   peripheral MMIO region
│           └── uart.rs         # BCM2711 PL011 UART driver
│
└── kernel/                 # Kernel binary crate
    ├── Cargo.toml          # Depends on arch + bsp; propagates bsp-* feature flags
    ├── link.ld             # Linker script: .text.boot at 0x80000, then .text,
    │                       #   .rodata, ALIGN(2M) __data_start (W^X boundary),
    │                       #   .data, .bss (16-byte aligned), .stack
    └── src/
        ├── main.rs         # kmain(): init UART/GIC/timer, mm::init(), tick verify, shell
        ├── panic.rs        # #[panic_handler]: print message + location, WFE halt
        ├── print.rs        # kprint!() / kprintln!() macros via core::fmt::Write
        ├── exceptions.rs   # IRQ dispatch (GIC acknowledge/EOI), sync exception
        │                   #   handler (SVC detection, ESR decoding), unhandled trap
        ├── shell.rs        # Interactive UART shell: help, uptime, ticks, info, mem,
        │                   #   svc, reboot
        └── mm/             # Memory management subsystem
            ├── mod.rs      # MM init: DTB RAM discovery → PMM → MMU enable → heap seed
            ├── dtb.rs      # Minimal FDT parser: extracts /memory node reg property
            ├── pmm.rs      # Bitmap page frame allocator: 1 bit per 4KB page, up to 4GB
            └── heap.rs     # Linked-list heap allocator: kmalloc/kfree, global stats
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
- **W^X memory policy** — no memory is simultaneously writable and executable.
  Code/rodata mapped as RO+X, data/BSS/heap/stack mapped as RW+NX, MMIO as RW+NX.
  The `__data_start` symbol is 2MB-aligned to match block descriptor granularity.

## QEMU Notes

- QEMU `raspi4b` starts at EL3, not EL2 — `boot.S` handles EL3→EL1 (secure, NS=0).
- The GIC-400 on QEMU doesn't reliably handle IGROUPR writes for PPIs from secure
  state, so all interrupts are kept as Group 0 (FIQEn=0 delivers them as IRQ).
- The virtual timer (CNTV, INTID 27) is used instead of the physical timer because
  CNTP doesn't fire from non-secure EL1 on QEMU's raspi4b.
- Real Pi 5 firmware enters at EL2 (non-secure) — `boot.S` handles EL2→EL1 directly.
