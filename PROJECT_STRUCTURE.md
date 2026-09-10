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
│   └── tiny_os_specification_v1.1.md
│
├── arch/                   # Architecture crate — hardware register access & HAL traits
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Crate root; re-exports arch-specific modules
│       ├── uart.rs         # UartDriver trait definition
│       ├── irq.rs          # InterruptController trait definition
│       ├── timer.rs        # Timer trait definition
│       └── aarch64/
│           ├── mod.rs      # AArch64 module root
│           ├── boot.S      # _start: park secondaries, EL3→EL1 (secure) or
│           │               #   EL2→EL1 drop, zero BSS, set SP, CPACR, VBAR, bl kmain
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries),
│           │               #   TrapFrame save/restore macros, handler stubs
│           ├── exceptions.rs # TrapFrame struct, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 (GICv2) driver: distributor + CPU interface
│           └── timer.rs    # ARM Generic Timer (virtual timer CNTV, 1kHz tick,
│                           #   CVAL-based acknowledge)
│
├── bsp/                    # Board Support Package crate — concrete HAL implementations
│   ├── Cargo.toml          # Features: bsp-rpi5 (default), bsp-qemu (mutually exclusive)
│   └── src/
│       ├── lib.rs          # Re-exports PlatformUart, GIC_DIST_BASE, GIC_CPU_BASE
│       │                   #   based on active feature flag
│       ├── rpi5/
│       │   ├── mod.rs          # BSP root for Raspberry Pi 5
│       │   ├── memory_map.rs   # RP1_UART0_BASE = 0x1F_0006_C000 (36-bit PCIe window),
│       │   │                   #   GIC bases at 0xFF841000 / 0xFF842000
│       │   └── rp1_uart.rs     # RP1 PL011 UART driver (MMIO volatile writes)
│       └── qemu_virt/
│           ├── mod.rs          # BSP root for QEMU raspi4b
│           ├── memory_map.rs   # UART at 0xFE20_1000, GIC bases at 0xFF841000 / 0xFF842000
│           └── uart.rs         # BCM2711 PL011 UART driver
│
└── kernel/                 # Kernel binary crate
    ├── Cargo.toml          # Depends on arch + bsp; propagates bsp-* feature flags
    ├── link.ld             # Linker script: .text.boot at 0x80000, then .text,
    │                       #   .rodata, .data, .bss (16-byte aligned), .stack
    └── src/
        ├── main.rs         # kmain(): init UART/GIC/timer, 250ms tick verify, shell
        ├── panic.rs        # #[panic_handler]: print message + location, WFE halt
        ├── print.rs        # kprint!() / kprintln!() macros via core::fmt::Write
        ├── exceptions.rs   # IRQ dispatch (GIC acknowledge/EOI), sync exception
        │                   #   handler (SVC detection, ESR decoding), unhandled trap
        └── shell.rs        # Interactive UART shell: help, uptime, ticks, info,
                            #   svc, reboot
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

## QEMU Notes

- QEMU `raspi4b` starts at EL3, not EL2 — `boot.S` handles EL3→EL1 (secure, NS=0).
- The GIC-400 on QEMU doesn't reliably handle IGROUPR writes for PPIs from secure
  state, so all interrupts are kept as Group 0 (FIQEn=0 delivers them as IRQ).
- The virtual timer (CNTV, INTID 27) is used instead of the physical timer because
  CNTP doesn't fire from non-secure EL1 on QEMU's raspi4b.
- Real Pi 5 firmware enters at EL2 (non-secure) — `boot.S` handles EL2→EL1 directly.
