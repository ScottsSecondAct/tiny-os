# tiny-os
[![Open Source](https://img.shields.io/badge/Open%20Source-Yes-green.svg)](https://github.com/ScottsSecondAct/some) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT) ![AI Assisted](https://img.shields.io/badge/AI%20Assisted-Claude-blue?logo=anthropic) [![Release](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/release.yml/badge.svg)](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/release.yml)

A bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). Designed for portability across ARM Cortex-A and Cortex-M families via a clean HAL trait layer. The specification (v1.2) includes RTOS certification provisions for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C, with WCET bounds, MC/DC coverage targets, health monitoring, watchdog integration, and mixed-criticality partitioning.

> **Note to CSU Sacramento CS students:** I wrote this project to show students what Claude Code can do and what a well-structured GitHub project can look like. If you're trying to land an internship or about to graduate, I strongly encourage you to put your projects on GitHub — it makes a real difference in your job search. Employers want to see what you can build, and a public repo is the easiest way to show them.
>
> This project took two days working with Claude Code to create. Your projects don't necessarily have to be this detailed, but don't be afraid to do a project. Use Claude Code, Cursor, Codex, etc. It's very satisfying to build a project and have it work. So think of something you're curious about and start. It doesn't have to be finished or working to put on GitHub — employers will be curious to see the process. Commit and push often.

## Status

**All 14 phases complete.** The full implementation spans bare-metal bootstrap, interrupts, memory management, multitasking, synchronization, driver framework, SMP, storage, filesystem, networking & user mode, safety certification, extended peripherals, security hardening, and advanced attack hardening. Highlights: 18 HAL traits, 21 syscalls with 19 capability bits, SMP across 4 Cortex-A76 cores, FAT32 filesystem, TCP/IP network stack, EL0 user-mode tasks with per-task page tables, ARMv8 Crypto Extensions, SDR104 UHS-I SD card, DVFS power management, allowlist firewall, persistent audit log, ARMv8.3 pointer authentication, secure memory wiping, syscall rate limiting, and RTOS certification provisions (WCET, MC/DC, health monitoring, mixed-criticality partitioning). 51 host tests, 15 QEMU integration checks, all 6 build configurations pass.

## Target Hardware

| Board | SoC | Notes |
|---|---|---|
| Raspberry Pi 5 | BCM2712 (4× Cortex-A76) | Primary target |
| Raspberry Pi 500 / CM5 | BCM2712 | Compatible |
| QEMU `-M raspi4b` | BCM2711 (Cortex-A72) | Development/CI |

UART, GPIO, SPI, I²C, and Ethernet are provided by the **RP1 southbridge**, connected via PCIe x4.

## Prerequisites

**Rust toolchain** (nightly, managed automatically via `rust-toolchain.toml`):

```sh
rustup toolchain install nightly
rustup target add aarch64-unknown-none
```

**Cross-compilation tools** (Debian/Ubuntu):

```sh
sudo apt install gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu
```

**QEMU** (for emulated testing):

```sh
sudo apt install qemu-system-aarch64
```

**cargo-binutils** (for producing the flat binary):

```sh
cargo install cargo-binutils
```

## Building & Running

### QEMU (quickest path)

```sh
make          # builds and launches QEMU raspi4b
# or equivalently:
make qemu
```

UART output appears on stdout. Type `help` at the `tiny_os>` prompt. Press `Ctrl-A X` to quit QEMU.

### Real Raspberry Pi 5

1. Build the flat binary:

   ```sh
   make img      # produces kernel8.img
   ```

2. Format a microSD card as FAT32 and copy the official Pi 5 firmware files
   (`bootcode.bin`, `start4.elf`, `fixup4.dat`, `bcm2712-rpi-5-b.dtb`) from
   the [Raspberry Pi firmware repo](https://github.com/raspberrypi/firmware/tree/master/boot).

3. Copy `kernel8.img` and `config.txt` from this repo to the SD card root.

4. Insert the card, connect a USB-to-serial adapter to GPIO 14/15, open a
   terminal at **115200 8N1**, and power on.

### Testing

```sh
make test          # run all tests (host unit + QEMU integration)
make test-host     # host-side unit tests only (44 tests, no QEMU needed)
make test-qemu     # QEMU integration tests only (15 boot verification checks)
```

Host-side tests verify pure-logic algorithms (IPv4 checksum, Ethernet/MBR parsing, SHA-256/HMAC, CRC32) natively. QEMU tests boot the kernel and check serial output for expected subsystem initialization.

### Other make targets

| Target | Description |
|---|---|
| `make build` | Build ELF for Pi 5 (no objcopy) |
| `make img` | Build Pi 5 `kernel8.img` flat binary |
| `make test` | Run all tests (host + QEMU) |
| `make test-host` | Host-side unit tests (pure-logic algorithms) |
| `make test-qemu` | QEMU integration tests (boot verification) |
| `make check-entry` | Verify `_start` is at `0x80000` |
| `make clean` | Remove build artifacts |

## Project Layout

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for a full annotated tree.

```
tiny_os/
├── arch/       # AArch64 boot, exception vectors, GIC-400, timer, MMU, mailbox, context switch, HAL traits
├── bsp/        # Board support: Pi 5 RP1 UART, QEMU PL011 UART, memory maps
├── examples/   # User-space applications (EL0): temp monitor, sensor gateway
├── kernel/     # Kernel entry, scheduler, sync, klog, watchdog, health, storage, fs (FAT32), net stack, firewall, crypto (SHA-256, HMAC, CRC32), integrity, audit, JTAG lockdown, PAC, syscall rate limiting, secure wipe, syscalls, user tasks, ELF loader, netbuf, IRQ dispatch, memory mgmt, shell
├── tests/      # Host unit tests (cargo test) + QEMU integration tests (boot verification)
└── docs/       # Specifications, API reference, and developer guides
```

## Documentation

- [User-Space App Guide](docs/USER_APP_GUIDE.md) — how to write, build, and run EL0 applications (syscalls, static vs dynamic loading, constraints, debugging)
- [API Reference](docs/API_REFERENCE.md) — shell commands, syscall table, scheduler, sync primitives, filesystem, network stack, HAL traits
- [Project Structure](PROJECT_STRUCTURE.md) — annotated source tree with every file and module

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 14-phase implementation plan and long-term goals.

## License

MIT — see [LICENSE](LICENSE).
