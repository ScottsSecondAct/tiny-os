# tiny_os — convenience build/test wrapper
#
# Usage:
#   make          — build for QEMU and launch QEMU (default)
#   make qemu     — same as above
#   make build    — build for real Pi 5 (release)
#   make img      — build Pi 5 kernel8.img (flat binary)
#   make test     — run all tests (host unit + QEMU integration)
#   make test-host — run host-side unit tests only
#   make test-qemu — run QEMU integration tests only
#   make clean    — cargo clean

CARGO          := cargo
OBJCOPY        := rust-objcopy
QEMU           := qemu-system-aarch64

RELEASE_FLAGS  := --release
QEMU_FLAGS     := --no-default-features --features kernel/bsp-qemu
QEMU_MACHINE   := -M raspi4b -serial stdio -display none -no-reboot
KERNEL_ELF     := target/aarch64-unknown-none/release/kernel
KERNEL_IMG     := kernel8.img

.PHONY: all qemu build img test test-host test-qemu clean

all: qemu

## Build for QEMU (raspi4b, BCM2711 GIC-400 + PL011 UART) and run.
qemu: _build_qemu
	$(QEMU) $(QEMU_MACHINE) -kernel $(KERNEL_ELF)

_build_qemu:
	$(CARGO) build $(RELEASE_FLAGS) $(QEMU_FLAGS)

## Build for real Pi 5 hardware (default BSP = bsp-rpi5).
build:
	$(CARGO) build $(RELEASE_FLAGS)

## Produce kernel8.img flat binary for SD card boot.
img: build
	$(OBJCOPY) -O binary $(KERNEL_ELF) $(KERNEL_IMG)
	@echo "  -> $(KERNEL_IMG)"

## Verify _start symbol address (should be 0x80000).
check-entry:
	rust-nm $(KERNEL_ELF) | grep _start

## Run all tests (host unit tests + QEMU integration tests).
test: test-host test-qemu

## Run host-side unit tests (pure-logic algorithms, no QEMU needed).
## Explicit --target overrides the workspace default (aarch64-unknown-none).
test-host:
	$(CARGO) test -p host-tests --target x86_64-pc-windows-msvc

## Run QEMU integration tests (boots kernel, checks serial output).
test-qemu:
	pwsh tests/qemu/run_tests.ps1

clean:
	$(CARGO) clean
	rm -f $(KERNEL_IMG)
