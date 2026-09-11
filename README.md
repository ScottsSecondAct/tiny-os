# tiny-os
[![Open Source](https://img.shields.io/badge/Open%20Source-Yes-green.svg)](https://github.com/ScottsSecondAct/some) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT) ![AI Assisted](https://img.shields.io/badge/AI%20Assisted-Claude-blue?logo=anthropic) [![Release](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/release.yml/badge.svg)](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/release.yml)

A bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). Designed for portability across ARM Cortex-A and Cortex-M families via a clean HAL trait layer. The specification (v1.2) includes RTOS certification provisions for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C, with WCET bounds, MC/DC coverage targets, health monitoring, watchdog integration, and mixed-criticality partitioning.

## Status

**Phase 10 complete** — Networking & User Mode: zero-copy network stack (Ethernet, ARP, IPv4, ICMP, UDP, TCP) with loopback device for QEMU, BSD socket API, EL0 user-mode tasks with per-task page tables (TTBR0+ASID), SVC-based syscall dispatch with subsystem multiplexing (FS, NET, SPI, I2C, GPIO). RP1 southbridge drivers for SPI (DW_apb_ssi), I2C (DW_apb_i2c), and GPIO (28-pin, pad control). User-space apps in `examples/`: temperature monitor (SoC temp stats), industrial sensor gateway (SPI/I2C/GPIO data collection, SD card logging, UDP telemetry). Optional ELF64 dynamic loader (`dynamic-load` feature) loads PIE binaries from the filesystem at runtime via `exec` shell command — disabled by default for safety-critical builds. Shell commands: `ping`, `netstat`, `ifconfig`, `temp`, `exec`. Built on Phase 9's filesystem, Phase 8's netbuf/block cache, Phase 7's SMP (4 cores), and earlier phases (scheduler, sync, MMU, GIC, timer).

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
make test-host     # host-side unit tests only (28 tests, no QEMU needed)
make test-qemu     # QEMU integration tests only (13 boot verification checks)
```

Host-side tests verify pure-logic algorithms (IPv4 checksum, Ethernet/MBR parsing) natively. QEMU tests boot the kernel and check serial output for expected subsystem initialization.

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
├── kernel/     # Kernel entry, scheduler, sync, klog, watchdog, health, storage, fs (FAT32), net stack, syscalls, user tasks, ELF loader, netbuf, IRQ dispatch, memory mgmt, shell
├── tests/      # Host unit tests (cargo test) + QEMU integration tests (boot verification)
└── docs/       # Specifications, API reference, and developer guides
```

## Documentation

- [User-Space App Guide](docs/USER_APP_GUIDE.md) — how to write, build, and run EL0 applications (syscalls, static vs dynamic loading, constraints, debugging)
- [API Reference](docs/API_REFERENCE.md) — shell commands, syscall table, scheduler, sync primitives, filesystem, network stack, HAL traits
- [Project Structure](PROJECT_STRUCTURE.md) — annotated source tree with every file and module

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 12-phase implementation plan (Phase 11: Safety Certification, Phase 12: Extended Peripheral Support).

## License

MIT — see [LICENSE](LICENSE).
