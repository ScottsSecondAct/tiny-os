# tiny_os — convenience build/test wrapper
#
# Usage:
#   make          — build for QEMU and launch QEMU (default)
#   make qemu     — same as above
#   make build    — build for real Pi 5 (release)
#   make img      — build Pi 5 kernel8.img (flat binary)
#   make clean    — cargo clean

CARGO          := cargo
OBJCOPY        := cargo objcopy
QEMU           := qemu-system-aarch64

RELEASE_FLAGS  := --release
QEMU_FLAGS     := --no-default-features --features kernel/bsp-qemu
QEMU_MACHINE   := -M raspi3b -serial stdio -display none -no-reboot
KERNEL_ELF     := target/aarch64-unknown-none/release/kernel
KERNEL_IMG     := kernel8.img

.PHONY: all qemu build img clean

all: qemu

## Build for QEMU (raspi3b, BCM2837 PL011 UART) and run.
qemu: _build_qemu
	$(QEMU) $(QEMU_MACHINE) -kernel $(KERNEL_ELF)

_build_qemu:
	$(CARGO) build $(RELEASE_FLAGS) $(QEMU_FLAGS)

## Build for real Pi 5 hardware (default BSP = bsp-rpi5).
build:
	$(CARGO) build $(RELEASE_FLAGS)

## Produce kernel8.img flat binary for SD card boot.
img: build
	$(OBJCOPY) $(RELEASE_FLAGS) -- -O binary $(KERNEL_IMG)
	@echo "  -> $(KERNEL_IMG)"

## Verify _start symbol address (should be 0x80000).
check-entry:
	aarch64-linux-gnu-nm $(KERNEL_ELF) | grep _start

clean:
	$(CARGO) clean
	rm -f $(KERNEL_IMG)
