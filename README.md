# tiny_os
[![Open Source](https://img.shields.io/badge/Open%20Source-Yes-green.svg)](https://github.com/ScottsSecondAct/some) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT) ![AI Assisted](https://img.shields.io/badge/AI%20Assisted-Claude-blue?logo=anthropic)[![CI](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/ci.yml/badge.svg)](https://github.com/ScottsSecondAct/tiny-os/actions/workflows/ci.yml)

A bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). Designed for portability across ARM Cortex-A and Cortex-M families via a clean HAL trait layer.

## Status

**Phase 1 complete** — bare-metal boot, UART console, and `kprint!` macros working on both QEMU and real Pi 5 hardware.

## Target Hardware

| Board | SoC | Notes |
|---|---|---|
| Raspberry Pi 5 | BCM2712 (4× Cortex-A76) | Primary target |
| Raspberry Pi 500 / CM5 | BCM2712 | Compatible |
| QEMU `-M raspi3b` | BCM2837 (Cortex-A53) | Development/CI |

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
make          # builds and launches QEMU raspi3b
# or equivalently:
make qemu
```

UART output appears on stdout. Press `Ctrl-A X` to quit QEMU.

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

### Other make targets

| Target | Description |
|---|---|
| `make build` | Build ELF for Pi 5 (no objcopy) |
| `make img` | Build Pi 5 `kernel8.img` flat binary |
| `make check-entry` | Verify `_start` is at `0x80000` |
| `make clean` | Remove build artifacts |

## Project Layout

See [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) for a full annotated tree.

```
tiny_os/
├── arch/       # AArch64 boot assembly, HAL trait definitions
├── bsp/        # Board support: Pi 5 RP1 UART, QEMU PL011 UART
├── kernel/     # Kernel entry point, print macros, panic handler
└── docs/       # Specifications and phase breakdown
```

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 10-phase implementation plan.

## License

MIT — see [LICENSE](LICENSE).
