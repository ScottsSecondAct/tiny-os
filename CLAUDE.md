# tiny_os — Project Context for Claude Code

## What This Is

tiny_os is a bare-metal real-time operating system written in Rust, targeting the Raspberry Pi 5 (BCM2712 SoC, quad Cortex-A76, GIC-400). It is designed for portability to other ARM cores (Cortex-A and Cortex-M families). The specification (v1.2) includes RTOS certification provisions for IEC 61508 SIL-2, ISO 26262 ASIL-B, and DO-178C DAL-C — covering WCET bounds, MC/DC coverage, health monitoring, watchdog integration, structured shutdown, mixed-criticality partitioning, and requirements traceability. The full specifications and implementation phases are in `docs/`.

## Current Phase

**Phase 12: Extended Peripheral Support** — complete. 5 new HAL traits (PwmDevice, SerialPort, RtcDevice, UsbHostController, CryptoEngine). 4 new RP1 BSP drivers (PWM, Serial/UART1-5, USB xHCI skeleton, Ethernet MAC skeleton). Software RTC with monotonic clock, power management via VideoCore mailbox DVFS. ARMv8 Crypto Extensions driver (AES ECB/CBC/CTR with hardware AESE/AESD instructions, software SHA-256). SDR104 UHS-I SD card upgrade with ADMA2 DMA. 7 new syscalls (SYS_UART=15 through SYS_POWER=21) with 7 capability bits (CAP_UART through CAP_POWER). Shell commands: `pwm`, `rtc`, `power`, `crypto`, `uart`. 51 host tests, all 6 build configurations pass.

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
- **Timer:** ARM Generic Timer (virtual timer CNTV_*_EL0 on QEMU, physical CNTP_*_EL0 on Pi 5), frequency from CNTFRQ_EL0
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
| `PageAllocator`      | `arch::mm`       | 3     | Physical page frame management        |
| `Context`            | `arch::context`  | 4     | Task context save/restore/switch      |
| `SmpBoot`            | `arch::smp`      | 7     | Multi-core startup                   |
| `DmaEngine`          | `arch::dma`      | 8     | DMA transfers                        |
| `BlockDevice`        | `drivers::block`  | 8     | Sector read/write                    |
| `NetDevice`          | `drivers::net`    | 10    | Packet TX/RX                         |
| `UserContext`         | `arch::user`     | 10    | EL0 task isolation, syscalls          |
| `SpiDevice`           | `arch::spi`      | 10.5  | SPI bus configure/transfer            |
| `I2cDevice`           | `arch::i2c`      | 10.5  | I2C bus configure/read/write          |
| `GpioController`      | `arch::gpio`     | 10.5  | GPIO pin mode/read/write              |
| `PwmDevice`           | `arch::pwm`      | 12    | PWM channel configure/enable/disable  |
| `SerialPort`          | `arch::serial`   | 12    | UART port open/read/write/close       |
| `RtcDevice`           | `arch::rtc`      | 12    | RTC get/set time, alarms              |
| `UsbHostController`   | `arch::usb`      | 12    | USB device enumeration/transfers      |
| `CryptoEngine`        | `arch::crypto_engine` | 12 | AES encrypt/decrypt, SHA-256          |

## Cargo Workspace Layout

```
tiny_os/
├── Cargo.toml              # Workspace root (default-members exclude tests/host)
├── CLAUDE.md               # This file
├── Makefile                # Build/test wrapper: make, make test, make test-host, make test-qemu
├── docs/                   # Specifications and guides (markdown)
│   ├── spec.md             # tiny_os system specification v1.1
│   ├── scheduler_spec.md   # Scheduler subsystem specification
│   ├── phases.md           # Implementation phases breakdown
│   ├── API_REFERENCE.md    # Complete API reference (shell, syscalls, HAL traits)
│   └── USER_APP_GUIDE.md   # Developer's guide for writing user-space applications
├── examples/               # User-space applications (run at EL0 via syscalls)
│   ├── temp_monitor/
│   │   ├── main.rs         # Temperature monitor: reads SoC temp via SYS_TEMPERATURE,
│   │   │                   #   tracks min/max/avg, prints periodic status (all .user.text)
│   │   └── README.md       # Description and sample output
│   └── sensor_gateway/
│       ├── main.rs         # Industrial sensor gateway: collects SPI/I2C/GPIO data,
│       │                   #   logs to SD card, forwards over UDP (all .user.text)
│       └── README.md       # Description, syscall usage, scheduling context
├── kernel/                 # Main kernel binary crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs         # kmain() entry point
│   │   ├── panic.rs        # panic_handler
│   │   ├── print.rs        # kprint!() / kprintln!() macros
│   │   ├── exceptions.rs   # IRQ dispatch, sync/SVC handler, unhandled trap
│   │   ├── shell.rs        # Interactive UART shell (help, uptime, ticks, info, mem, tasks, log, health, smp, sd, sdread, ls, cat, hexdump, touch, write, ping, netstat, ifconfig, temp, firewall, integrity, audit, pwm, rtc, power, crypto, uart, exec [dynamic-load], faulttest, wcet, yield, svc, reboot)
│   │   ├── netbuf.rs       # Zero-copy DMA buffer pool: 1024×1536B buffers in NC memory
│   │   ├── syscall.rs      # Syscall dispatch: basic (SYS_YIELD..SYS_TEMPERATURE),
│   │   │                   #   subsystem multiplexed (SYS_FS, SYS_NET, SYS_SPI, SYS_I2C,
│   │   │                   #   SYS_GPIO), and Phase 12 peripherals (SYS_UART, SYS_PWM,
│   │   │                   #   SYS_RTC, SYS_DMA, SYS_USB, SYS_CRYPTO, SYS_POWER)
│   │   ├── periph.rs       # Peripheral driver instances (SPI/I2C/GPIO): cfg-gated
│   │   │                   #   RP1 drivers on Pi 5, stubs returning errors on QEMU
│   │   ├── user_tasks.rs   # EL0 user demo task with inline-asm syscall stubs (.user.text section)
│   │   ├── loader.rs       # [dynamic-load] ELF64 loader: parse headers, load PT_LOAD segments,
│   │   │                   #   apply R_AARCH64_RELATIVE relocations, create per-task page tables
│   │   ├── os_cfg.rs       # Centralized OS_CFG_* constants with 14 compile-time assertions
│   │   ├── hooks.rs        # 14 os_hook_* functions with default behaviors (spec 11.2)
│   │   ├── shutdown.rs     # Structured shutdown: diag save, fault log, hook, reboot/halt
│   │   ├── criticality.rs  # Criticality mode switch: suspend/restore lower-priority tasks
│   │   ├── wcet.rs         # WCET measurement harness: PMU cycle counter, min/max/avg tracking
│   │   ├── sched_analysis.rs # RMA utilization check + Response-Time Analysis with PIP blocking
│   │   ├── fault_inject.rs # Fault injection test suite: 8 tests for safety certification
│   │   ├── rtc.rs          # Software RTC: AtomicU64 epoch, monotonic clock, alarm
│   │   ├── power.rs        # Power management: DVFS via VideoCore mailbox, WFI idle
│   │   ├── crypto/         # Cryptographic primitives subsystem
│   │   │   ├── mod.rs      # Module declarations (sha256, hmac, crc32, hw)
│   │   │   ├── sha256.rs   # SHA-256 (FIPS 180-4): runtime hash + const fn for compile-time
│   │   │   ├── hmac.rs     # HMAC-SHA256 (RFC 2104): constant-time comparison
│   │   │   ├── crc32.rs    # CRC32 with precomputed 256-entry lookup table
│   │   │   └── hw.rs       # ARMv8 Crypto Extensions: AES (ECB/CBC/CTR), software SHA-256
│   │   ├── integrity.rs    # Runtime code integrity: CRC32 of .text at boot, periodic verify
│   │   ├── audit.rs        # Audit log: 64-entry ring buffer, 10 event types, FAT32 persist
│   │   ├── jtag.rs         # JTAG/debug lockdown: OSLAR_EL1, GPIO 22-27 disable
│   │   ├── net/            # Network stack subsystem
│   │   │   ├── mod.rs      # Network init, RX dispatch, net_task poll loop
│   │   │   ├── ethernet.rs # Ethernet frame parse/build (14-byte header, EtherType)
│   │   │   ├── arp.rs      # ARP cache (16 entries) + request/reply handling
│   │   │   ├── ipv4.rs     # IPv4 parse/build (20-byte header), internet checksum
│   │   │   ├── icmp.rs     # ICMP echo request/reply, ping RTT tracking
│   │   │   ├── udp.rs      # UDP parse/build (8-byte header), port table
│   │   │   ├── tcp.rs      # Minimal TCP state machine (client SYN/ACK/FIN, 4 connections)
│   │   │   ├── socket.rs   # BSD socket API: socket/bind/connect/sendto/recvfrom/close
│   │   │   ├── loopback.rs # Loopback NetDevice for QEMU (swaps src/dst, ICMP req→reply)
│   │   │   └── firewall.rs # Allowlist firewall: 16-rule table, default-deny, per-packet filter
│   │   ├── fs/             # Filesystem subsystem
│   │   │   ├── mod.rs      # VFS: FsError, 16-entry fd table, open/read/write/close/readdir API
│   │   │   └── fat32.rs    # FAT32: BPB, FAT chain, dir parsing, LFN, read/write, create
│   │   ├── storage/        # Storage subsystem
│   │   │   ├── mod.rs      # Device routing (EMMC2/RamDisk), cached read/write, flush
│   │   │   ├── ramdisk.rs  # RAM-backed BlockDevice (256 KB, FAT32-formatted for QEMU)
│   │   │   ├── mbr.rs      # MBR partition table parser
│   │   │   └── cache.rs    # LRU write-back block cache (32 lines, 512B each)
│   │   ├── sched.rs        # SMP-aware 256-level fixed-priority scheduler: per-core current, spinlock, IPI, TTBR0 swap
│   │   ├── spinlock.rs     # Ticket spinlock with IRQ save/restore for SMP mutual exclusion
│   │   ├── klog.rs         # Ring-buffer log subsystem: 5 levels, timestamps, module tags, 64-entry buffer
│   │   ├── watchdog.rs     # Software watchdog: tick-based counter, auto-kick task at priority 0
│   │   ├── health.rs       # Health monitor task: stack watermarks, CPU utilization, watchdog status
│   │   ├── drivers.rs      # Driver trait (probe/remove lifecycle) and static registry
│   │   ├── sync/           # Synchronization primitives subsystem
│   │   │   ├── mod.rs      # WaitQueue: priority-sorted waiter array with lazy cleanup
│   │   │   ├── mutex.rs    # Mutex with PIP, PCP, recursive locking, timeout
│   │   │   ├── semaphore.rs # Counting/binary semaphore with timeout
│   │   │   ├── events.rs   # 32-bit event flags with Any/All wait modes
│   │   │   └── msgqueue.rs # Const-generic message queue with send/recv blocking
│   │   └── mm/             # Memory management subsystem
│   │       ├── mod.rs      # MM init: RAM discovery, PMM, DMA pool, MMU enable, heap seeding
│   │       ├── dtb.rs      # Minimal FDT parser for /memory node
│   │       ├── pmm.rs      # Bitmap page frame allocator (4KB pages, up to 4GB)
│   │       ├── heap.rs     # Linked-list heap allocator (kmalloc/kfree, disabled in safety-critical)
│   │       └── pool.rs     # Fixed-size memory pool allocator (O(1) alloc/free, up to 16 pools)
│   └── link.ld             # Linker script (.text.boot, .text, .rodata, .user.text at 0x200000, .data, .bss, .stack)
├── arch/                   # Architecture-specific crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── uart.rs         # UartDriver trait
│       ├── irq.rs          # InterruptController trait
│       ├── timer.rs        # Timer trait
│       ├── mm.rs           # PageAllocator trait
│       ├── context.rs      # Context HAL trait (new_context, switch)
│       ├── smp.rs          # SmpBoot HAL trait (core_id, num_cores, start_core)
│       ├── block.rs        # BlockDevice HAL trait (sector read/write)
│       ├── dma.rs          # DmaEngine HAL trait (channel-based DMA transfers)
│       ├── net.rs          # NetDevice HAL trait (packet TX/RX)
│       ├── user.rs         # UserContext HAL trait (EL0 task isolation)
│       ├── spi.rs          # SpiDevice HAL trait (configure, transfer, read, write)
│       ├── i2c.rs          # I2cDevice HAL trait (configure, read, write, write_read)
│       ├── gpio.rs         # GpioController HAL trait (set_mode, set_pull, read, write)
│       ├── pwm.rs          # PwmDevice HAL trait (configure, set_duty, enable, disable)
│       ├── serial.rs       # SerialPort HAL trait (open, write, read, close)
│       ├── rtc.rs          # RtcDevice HAL trait (get/set time, alarm management)
│       ├── usb.rs          # UsbHostController HAL trait (enumerate, transfers)
│       ├── crypto_engine.rs # CryptoEngine HAL trait (AES encrypt/decrypt, SHA-256)
│       └── aarch64/
│           ├── mod.rs
│           ├── boot.S      # _start, spin-table secondary parking, secondary_boot EL drop
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries)
│           ├── exceptions.rs # TrapFrame, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 driver: distributor, CPU interface, SGI for IPI
│           ├── timer.rs    # ARM Generic Timer (virtual timer, 1kHz tick, secondary init)
│           ├── mmu.rs      # MMU: identity mapping, 2MB blocks, W^X, secondary core init,
│           │               #   per-task TTBR0 page tables (L3 4KB), ASID, switch_ttbr0
│           ├── smp.rs      # AArch64 SMP: spin-table wakeup, core_id, start_core
│           ├── mailbox.rs  # VideoCore mailbox driver: property tags, SoC temperature query
│           ├── emmc2.rs    # SDHCI/EMMC2 SD card driver: PIO mode, card init, read/write
│           ├── context.rs  # Aarch64Context: new_context (fake frame), new_user_context, switch wrapper
│           └── context_switch.S  # Context switch (x19-x30), task_trampoline (sched lock release),
│                           #   task_trampoline_user (eret to EL0)
├── bsp/                    # Board Support Packages
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # cfg-gated re-exports (PlatformUart, GIC bases, MAILBOX_BASE,
│       │                   #   Rp1Spi, Rp1I2c, Rp1Gpio, Rp1Pwm, Rp1Serial, Rp1Usb, Rp1Eth on bsp-rpi5)
│       ├── rpi5/
│       │   ├── mod.rs
│       │   ├── rp1_uart.rs
│       │   ├── rp1_spi.rs      # RP1 SPI0 driver (DW_apb_ssi, polling mode)
│       │   ├── rp1_i2c.rs      # RP1 I2C0 driver (DW_apb_i2c, polling mode)
│       │   ├── rp1_gpio.rs     # RP1 GPIO driver (28 pins, pad control, RIO)
│       │   ├── rp1_pwm.rs      # RP1 PWM driver (2 channels, 50 MHz ref clock)
│       │   ├── rp1_serial.rs   # RP1 UART1-5 driver (PL011, 48 MHz ref clock)
│       │   ├── rp1_usb.rs      # RP1 xHCI USB host skeleton (all ops return NotAvailable)
│       │   ├── rp1_eth.rs      # RP1 Ethernet MAC skeleton (Synopsys GMAC)
│       │   └── memory_map.rs   # RP1 UART, SPI0, I2C0, GPIO, PWM, ETH, USB bases,
│       │                       #   GIC, RAM, MAILBOX_BASE, peripheral + RP1 MMIO regions
│       └── qemu_virt/
│           ├── mod.rs
│           ├── uart.rs     # BCM2711 PL011 UART at 0xFE20_1000
│           └── memory_map.rs   # UART, GIC, RAM, MAILBOX_BASE, peripheral MMIO regions
└── tests/                  # Test infrastructure
    ├── host/               # Host-side unit tests (cargo test, runs natively)
    │   ├── Cargo.toml      # Separate crate; built with --target x86_64-pc-windows-msvc
    │   └── src/
    │       ├── lib.rs      # Crate root (declares test modules)
    │       ├── ipv4.rs     # IPv4 checksum + header parsing tests (12 tests)
    │       ├── ethernet.rs # Ethernet frame parsing tests (7 tests)
    │       ├── mbr.rs      # MBR partition table parsing tests (7 tests)
    │       ├── sha256.rs   # SHA-256, HMAC-SHA256 tests (11 tests, incl. RFC 4231)
    │       ├── crc32.rs    # CRC32 tests (6 tests, incl. check value 0xCBF43926)
    │       └── rtc.rs     # RTC datetime conversion tests (7 tests, epoch/leap/roundtrip)
    └── qemu/
        └── run_tests.ps1   # QEMU integration test runner (15 boot verification checks)
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

## Testing

Two-tier test infrastructure accommodates the bare-metal constraint (the workspace default target is `aarch64-unknown-none`, which has no `std`).

### Host-side unit tests (51 tests)

Pure-logic algorithms (checksums, parsers) re-implemented in `tests/host/` and tested natively with `cargo test`. The host-tests crate requires an explicit `--target` override because the workspace default target is bare-metal.

```sh
make test-host                # or: cargo test -p host-tests --target x86_64-pc-windows-msvc
```

### QEMU integration tests (15 checks)

Boots the kernel on QEMU `raspi4b`, captures serial output, and verifies expected patterns (kernel banner, MMU, timer, scheduler, SMP cores, network, filesystem, user mode, shell prompt, no panic).

```sh
make test-qemu                # or: pwsh tests/qemu/run_tests.ps1
```

### Run all tests

```sh
make test                     # runs test-host then test-qemu
```

## Completed Phases

### Phase 1 — Bare-Metal Bootstrap & UART Console ✅

- [x] Cargo workspace with `kernel`, `arch`, `bsp` crates
- [x] Linker script: kernel at 0x80000, sections: .text, .rodata, .data, .bss, .stack
- [x] `_start` in AArch64 assembly: park secondary cores (WFE), zero .bss, set SP, branch to kmain
- [x] RP1 UART driver (MMIO volatile writes to RP1 peripheral window)
- [x] `kprint!()` / `kprintln!()` macros via `core::fmt::Write`
- [x] `panic_handler` that prints message + location, then infinite WFE
- [x] Build pipeline: `cargo build` → `cargo objcopy` → `kernel8.img`
- [x] Boot test on QEMU (`-M raspi4b -serial stdio`) and real Pi 5 hardware

### Phase 2 — Interrupts & Timer ✅

- [x] Exception vector table (`vectors.S`) — 2KB aligned, 16 entries with TrapFrame save/restore
- [x] GIC-400 driver: distributor + CPU interface init, IRQ enable/disable/priority, EOI
- [x] `InterruptController` HAL trait
- [x] ARM Generic Timer driver — virtual timer (CNTV) with CVAL-based acknowledge for accurate 1kHz tick
- [x] `Timer` HAL trait with periodic tick and monotonic clock
- [x] System tick ISR incrementing a global tick counter (AtomicU64)
- [x] Sync exception handler — SVC detection with ELR advance, ESR decoding
- [x] Interactive shell: help, uptime, ticks, info, svc, reboot
- [x] Boot: EL3→EL1 (secure) on QEMU, EL2→EL1 on real Pi 5
- [x] Verified: 250 ticks in 250ms (±0.4%), uptime accurate to wall-clock

### Phase 3 — Memory Management ✅

- [x] Minimal FDT (DTB) parser for /memory node RAM discovery
- [x] Boot.S preserves firmware DTB pointer (x19 → DTB_PTR global)
- [x] Physical page frame allocator: bitmap-based, 4KB pages, up to 4GB
- [x] `PageAllocator` HAL trait in `arch::mm`
- [x] AArch64 MMU setup: 4KB granule, 2MB block descriptors, 48-bit VA
- [x] Identity mapping with W^X: code RO+X (RoCode), data RW+NX (Ram), MMIO RW+NX (Device)
- [x] MAIR (3 indices), TCR (40-bit IPS, EPD1), SCTLR (MMU + D-cache + I-cache)
- [x] Linker script `__data_start` symbol at 2MB boundary for clean W^X permission split
- [x] Linked-list heap allocator with `kmalloc`/`kfree`, seeded from PMM pages (256KB)
- [x] BSP memory region constants (RAM defaults, peripheral MMIO, RP1 window)
- [x] Shell `mem` command: page stats, heap stats, MMU status
- [x] Verified on QEMU: 262K pages, MMU+caches on, timer accuracy maintained

### Phase 4 — Multitasking & Context Switch ✅

- [x] Task Control Block (TCB) with saved context, priority, state, stack, delay timer
- [x] Five task states: Ready, Running, Blocked, Suspended, Dormant
- [x] AArch64 context switch: save/restore callee-saved registers (x19-x30), task trampoline with IRQ enable
- [x] `Context` HAL trait in `arch::context`
- [x] 256-level fixed-priority scheduler with O(1) dispatch (4×u64 bitmap + trailing_zeros)
- [x] Round-robin among equal-priority tasks via per-level FIFO linked-list queues
- [x] Preemption from timer tick ISR (`sched::tick()` called from IRQ handler)
- [x] `task_create`, `task_delete`, `task_suspend`, `task_resume`, `task_yield`, `delay` API
- [x] Critical sections: DAIF masking with RAII guard (CriticalSection)
- [x] Idle task at priority 255 (WFE loop)
- [x] Shell `tasks` command showing task list with ID, name, priority, state
- [x] Verified on QEMU: multi-task context switching, timer accuracy 249/250 ticks

### Phase 5 — Synchronization Primitives ✅

- [x] Mutex with Priority Inheritance Protocol (PIP) and Priority Ceiling Protocol (PCP)
- [x] Recursive mutex locking (max nest depth 8)
- [x] Binary and counting semaphores with configurable max count
- [x] 32-bit event flags with Any/All wait modes, up to 16 concurrent waiters
- [x] Const-generic message queue `MsgQueue<MSG_SIZE, CAPACITY>` with circular buffer
- [x] Timeout support on all blocking operations (lock_timeout, wait_timeout, send_timeout, recv_timeout)
- [x] Non-blocking try variants (try_lock, try_wait, try_send, try_recv)
- [x] WaitQueue: priority-ordered waiter array with lazy stale-entry cleanup
- [x] Scheduler extended with WaitResult enum, base_priority tracking, public sync APIs
- [x] Demo tasks: mutex-protected shared counter + binary semaphore signaling between tasks
- [x] Verified on QEMU: PIP mutex, semaphore sync, correct counter handoff across tasks

### Phase 6 — Driver Framework & Logging ✅

- [x] `klog` subsystem: 5 log levels (ERROR, WARN, INFO, DEBUG, TRACE), timestamps, module tags
- [x] 64-entry ring-buffer log drain, accessible via shell `log` command
- [x] Runtime log level filter (`log level <level>` shell command)
- [x] Error/Warn messages auto-print to UART; Info/Debug/Trace to ring buffer only
- [x] Per-task execution-time budget enforcement (`task_set_budget`, `task_get_remaining`, `task_reset_budget`)
- [x] Deadline-miss detection: klog warning when task budget exhausted
- [x] Task criticality levels: SafetyCritical, MissionCritical, Standard, BestEffort
- [x] Stack watermark tracking: canary fill (0xAA) at task creation, runtime measurement
- [x] Health monitoring task (priority 1): periodic stack/CPU/watchdog checks every 5 seconds
- [x] Software watchdog: tick-based counter with auto-kick task at priority 0
- [x] CPU utilization tracking: `sched::utilization()` returns busy/total ticks
- [x] Driver trait definition with probe/remove lifecycle and static registry (16 slots)
- [x] Shell commands: `log [N]`, `log level <level>`, `health` (stack watermarks, CPU, watchdog)
- [x] Extended `tasks` command showing criticality and CPU ticks per task
- [x] Verified on QEMU: 6 tasks (incl. watchdog-kick and health-mon), no timeouts, stable operation

### Phase 7 — Symmetric Multiprocessing (SMP) ✅

- [x] Spin-table secondary core wakeup in boot.S (release addresses in `.data` section)
- [x] Secondary boot path: EL3/EL2→EL1 drop, per-core stack, FP/SIMD, vectors, MMU, GIC CPU interface, timer
- [x] Per-core stacks (8KB each for secondary cores, 512KB for primary)
- [x] `SmpBoot` HAL trait (`arch::smp`) with `core_id()`, `num_cores()`, `start_core()`
- [x] AArch64 SMP implementation using spin-table + SEV wakeup
- [x] Ticket spinlock (`SpinLock`) with IRQ save/restore for SMP mutual exclusion
- [x] Global run queue with spinlock (shared 256-level ready queues across all cores)
- [x] Per-core current task tracking (`current: [u8; MAX_CORES]`) and per-core idle tasks
- [x] IPI via GIC SGI #0 for cross-core reschedule when idle cores should pick up work
- [x] Scheduler lock held across context_switch, released by resumed task (or task_trampoline for new tasks)
- [x] UART print serialization via spinlock (clean output across concurrent cores)
- [x] Shell `smp` command and `info` command showing core count and current core
- [x] Verified on QEMU: all 4 cores online, tasks migrating across cores 0-3, mutex/semaphore correct across cores

### Phase 8 — Storage & DMA ✅

- [x] `DmaEngine` HAL trait (`arch::dma`) with channel-based configure/start/complete/abort API
- [x] `BlockDevice` HAL trait (`arch::block`) with read_block/write_block/block_count/block_size
- [x] Zero-copy buffer pool (`kernel::netbuf`): 1024 × 1536B buffers in 2MB non-cacheable region at 0x0100_0000
- [x] `MemKind::NonCacheable` MMU support using MAIR index 2 (Normal NC)
- [x] Contiguous page allocator (`alloc_pages`) for DMA pool allocation
- [x] SDHCI/EMMC2 SD card driver (`arch::aarch64::emmc2`): full card init (CMD0/8/ACMD41/2/3/9/7), PIO read/write (CMD17/CMD24), CSD v1/v2 parsing
- [x] MBR partition table parser (`kernel::storage::mbr`): 4 entries, type identification, signature validation
- [x] LRU write-back block cache (`kernel::storage::cache`): 32 lines, dirty tracking, eviction with write-back
- [x] IRQ dispatch table enlarged from 64 to 256 entries for EMMC2 INTID support
- [x] Shell commands: `sd` (card info, partitions), `sdread <lba>` (sector hex dump), `mem` (netbuf stats)
- [x] Verified on QEMU: boots with graceful SD card detection (QEMU uses SDHOST, not SDHCI)
- [x] Both BSPs (QEMU and RPi5) build cleanly

### Phase 9 — Filesystem & Shell ✅

- [x] Storage layer routing (`kernel::storage::mod`) between EMMC2 and RamDisk with static BlockCache
- [x] RamDisk BlockDevice (`kernel::storage::ramdisk`): 256 KB `.bss` array formatted as FAT32 at init
- [x] VFS layer (`kernel::fs::mod`): 16-entry fd table, open/read/write/close/readdir_open/readdir_next API
- [x] FAT32 driver (`kernel::fs::fat32`): BPB parsing, FAT chain traversal, directory parsing with LFN support
- [x] Path resolution: case-insensitive name matching, nested directory support
- [x] File read: sequential read with cluster chain caching, sector-by-sector through block cache
- [x] File write: cluster chain extension via alloc_cluster, read-modify-write for partial sectors
- [x] File create: 8.3 short name generation, free directory entry scan, cluster allocation
- [x] Directory entry size update on close for writable files
- [x] PL011 UART FIFO enabled for reliable serial RX
- [x] Shell FIFO drain: burst-read all available UART bytes before yielding
- [x] Shell commands: `ls [path]`, `cat <path>`, `hexdump <path>`, `touch <path>`, `write <path> <text>`
- [x] SMP timeout: hardware counter-based 3-second deadline for secondary core wakeup
- [x] Verified on QEMU: ramdisk mount, ls/cat/hexdump/touch/write all functional, all 4 cores online
- [x] Both BSPs (QEMU and RPi5) build cleanly

### Phase 10 — Networking & User Mode ✅

- [x] `NetDevice` HAL trait (`arch::net`) with send/recv/mac_addr for zero-copy packet I/O
- [x] `UserContext` HAL trait (`arch::user`) for EL0 task creation
- [x] Network stack: Ethernet frame parse/build, ARP cache (16 entries), IPv4 with checksum, ICMP echo, UDP, minimal TCP (client SYN/ACK/FIN)
- [x] BSD socket API: socket/bind/connect/sendto/recvfrom/close (8-entry socket table)
- [x] Loopback NetDevice for QEMU testing (swaps src/dst, converts ICMP request→reply)
- [x] Network task polling loop with loopback device init (IP 127.0.0.1)
- [x] Per-task TTBR0 page tables: clone kernel L0/L1/L2, L3 4KB pages for user stacks with guard page
- [x] ASID-tagged address spaces (8-bit ASID per user task, TLBI on switch)
- [x] `task_trampoline_user` in assembly: releases sched lock, sets SPSR_EL1=0 (EL0t), erets to user entry
- [x] `task_create_user` API: kernel stack + user stack + TTBR0 allocation
- [x] TTBR0 swap in scheduler on context switch between tasks with different page tables
- [x] Syscall dispatch via SVC #0: X8=syscall number, X0-X1=args, X0=return value
- [x] Seven basic syscalls: SYS_YIELD(0), SYS_DELAY(1), SYS_WRITE(2), SYS_TASK_ID(3), SYS_UPTIME(4), SYS_EXIT(5), SYS_TEMPERATURE(6)
- [x] Subsystem multiplexed syscalls: SYS_FS(10), SYS_NET(11), SYS_SPI(12), SYS_I2C(13), SYS_GPIO(14) with X0=operation, X1-X3=args
- [x] `.user.text` linker section at 0x200000 (2MB-aligned) with EL0-accessible permissions
- [x] User demo task: prints via sys_write, delays via sys_delay, runs indefinitely at EL0
- [x] EL0 fault handling: data/prefetch abort from EL0 logs registers and terminates task
- [x] DISCARD_SP pattern for task_terminate context switch (avoids cascading faults)
- [x] VideoCore mailbox driver (`arch::aarch64::mailbox`): property tag interface, SoC temperature query (tag 0x00030006)
- [x] `SpiDevice` HAL trait (`arch::spi`), `I2cDevice` HAL trait (`arch::i2c`), `GpioController` HAL trait (`arch::gpio`)
- [x] RP1 SPI0 driver (`bsp::rpi5::rp1_spi`): DW_apb_ssi, polling mode, 200 MHz ref clock
- [x] RP1 I2C0 driver (`bsp::rpi5::rp1_i2c`): DW_apb_i2c, standard/fast mode, 200 MHz ref clock
- [x] RP1 GPIO driver (`bsp::rpi5::rp1_gpio`): 28 pins, pad control, RIO set/clr registers
- [x] Kernel peripheral manager (`kernel::periph`): cfg-gated RP1 driver instances on Pi 5, error stubs on QEMU
- [x] User-space temperature monitor (`examples/temp_monitor/main.rs`): EL0 app reading SoC temperature via SYS_TEMPERATURE syscall, min/max/avg stats, 5s periodic output
- [x] User-space sensor gateway (`examples/sensor_gateway/main.rs`): EL0 app collecting SPI/I2C/GPIO sensor data, SD card logging, UDP telemetry forwarding
- [x] Shell commands: `ping <ip>`, `netstat` (ARP/socket/config), `ifconfig` (IP/MAC/link), `temp`
- [x] Verified on QEMU: loopback ping, user task at EL0, syscalls, temperature monitor, sensor gateway (graceful hw fallback), no faults, stable operation
- [x] Both BSPs (QEMU and RPi5) build cleanly, all 4 BSP×feature configurations pass

### Phase 11 — Safety Certification ✅

- [x] `os_cfg` module with compile-time validation (14 const assertions for all tunable constants)
- [x] `safety-critical` Cargo feature flag: disables heap (pool-only allocation), enforces budget monitoring
- [x] Fixed-size memory pool allocator (`OsPool`): O(1) alloc/free via spinlock-protected free-list, up to 16 pools, global registry for diagnostics
- [x] 14 hook functions with default behaviors: idle (WFI), stack_overflow, data_abort, hard_fault, assert, task_create, task_switch, budget_overrun, deadline_miss, task_terminated, watchdog_expired, health_check_failed, shutdown, safety_critical_lost
- [x] Enhanced health monitor: 7 checks (stacks with hook call, CPU utilization, watchdog, ready queue integrity, mutex ownership, tick monotonicity, pool accounting)
- [x] Budget enforcement: task suspension on overrun when BUDGET_EN, period-based replenishment, os_hook_budget_overrun callback
- [x] Structured shutdown: interrupt mask (DAIF), diagnostic register save (DiagRegion: ESR, ELR, FAR, SPSR, regs, tick, core_id, task_id), fault logging, hook call, reboot-or-halt based on REBOOT_ON_FAULT
- [x] Criticality mode switch: os_criticality_switch suspends tasks below min_level, os_criticality_restore resumes all
- [x] WCET measurement harness: PMU cycle counter (PMCCNTR_EL0), WcetRecord with min/max/avg/count, 32 slots, WCET bound constants
- [x] Schedulability analysis: RMA utilization check with precomputed Liu & Layland bounds, iterative Response-Time Analysis with PIP blocking
- [x] Fault injection test suite: 8 tests (pool exhaust/double-free/bad-ptr, budget overrun, health hooks, criticality switch, diag region, hook invocation)
- [x] Requirements traceability matrix: 86 requirements in docs/traceability.csv (REQ-CFG, REQ-POOL, REQ-SAFE, REQ-HOOK, REQ-HEALTH, REQ-SHUTDOWN, REQ-CRIT, REQ-WCET, REQ-BUDGET, REQ-SCHED, REQ-SEC-FW, REQ-SEC-CRYPTO, REQ-SEC-AUTH, REQ-SEC-CAP, REQ-SEC-INT, REQ-SEC-AUDIT, REQ-SEC-JTAG, REQ-SEC-HEALTH)
- [x] Shell commands: `faulttest` (run fault injection suite), `wcet` (dump WCET measurements)
- [x] All 6 BSP×feature configurations build cleanly, all 28 host tests pass

### Phase 13 — Security Hardening ✅

- [x] Allowlist-based network firewall (`kernel::net::firewall`): 16-rule table, default-deny when enabled, per-packet source IP/mask + destination port + protocol matching, atomic pass/drop counters
- [x] SHA-256 (FIPS 180-4) in `kernel::crypto::sha256`: runtime `hash()` and `Sha256` init/update/finalize, plus `const fn const_hash()` for compile-time password hashing
- [x] HMAC-SHA256 (RFC 2104) in `kernel::crypto::hmac`: `hmac_sha256()`, `verify()` with constant-time comparison to prevent timing attacks
- [x] CRC32 in `kernel::crypto::crc32`: precomputed 256-entry lookup table, `crc32()` and incremental `crc32_update()`
- [x] Shell authentication (`shell.rs`): SHA-256 password hash verified at compile time, 3-attempt lockout with 30s delay, cfg-gated via `os_cfg::SHELL_AUTH_EN` (auto-enabled in safety-critical mode)
- [x] Per-task syscall capability bitmask: `capabilities: u32` in TCB, 12 capability bits (CAP_YIELD through CAP_GPIO), `CAP_ALL` for kernel tasks, restricted `CAP_USER_DEFAULT` (excludes SPI/I2C/GPIO) for user tasks
- [x] Capability enforcement in syscall dispatch: `cap_for_syscall()` maps syscall number to required capability bit, `task_has_capability()` check before dispatch, `E_PERM` on denial with audit log
- [x] Runtime code integrity (`kernel::integrity`): CRC32 of `.text` section (from `_start` to `__data_start`) computed at boot, periodic re-verification by health monitor
- [x] Persistent audit log (`kernel::audit`): 64-entry ring buffer, 10 event types (Boot, Shutdown, AuthOk/Fail, FirewallDrop, CapabilityDenied, IntegrityOk/Fail, TaskCreated/Terminated), per-entry tick/core/task, `persist_to_fs()` writes to FAT32 `/audit.log`
- [x] Health monitor extended with code integrity check (8 checks total: stacks, CPU, watchdog, ready queue, mutex ownership, tick monotonicity, pool accounting, code integrity)
- [x] JTAG/debug lockdown (`kernel::jtag`): OSLAR_EL1 debug register lock, GPIO 22-27 reconfigured to input+pull-down on Pi 5 in safety-critical mode
- [x] Security configuration constants in `os_cfg`: MAX_FIREWALL_RULES, SHELL_AUTH_EN, SHELL_AUTH_MAX_ATTEMPTS, SHELL_AUTH_LOCKOUT_MS, DEBUG_LOCKDOWN
- [x] Shell commands: `firewall` (status, rules, counters), `integrity` (CRC32 status, check count), `audit [N]` / `audit persist` (view/persist log)
- [x] Host-side tests: SHA-256 (6 tests incl. NIST vectors), HMAC-SHA256 (4 tests incl. RFC 4231), CRC32 (6 tests incl. check value), plus existing 28 = 44 total
- [x] All 6 BSP×feature configurations build cleanly, all 44 host tests pass

### Phase 12 — Extended Peripheral Support ✅

- [x] 5 new HAL traits: `PwmDevice` (`arch::pwm`), `SerialPort` (`arch::serial`), `RtcDevice` (`arch::rtc`), `UsbHostController` (`arch::usb`), `CryptoEngine` (`arch::crypto_engine`)
- [x] RP1 PWM driver (`bsp::rpi5::rp1_pwm`): 2-channel, 50 MHz reference clock, frequency/duty calculation, MSEN mode
- [x] RP1 Serial driver (`bsp::rpi5::rp1_serial`): UART1-5 (PL011-compatible, 48 MHz ref, baud rate divisor, 800-byte stride)
- [x] RP1 USB xHCI skeleton (`bsp::rpi5::rp1_usb`): register map comments, all operations return `NotAvailable`
- [x] RP1 Ethernet MAC skeleton (`bsp::rpi5::rp1_eth`): Synopsys GMAC register map, `NetDevice` trait returning errors
- [x] BSP memory map: `RP1_PWM_BASE`, `RP1_UART1_BASE`, `RP1_ETH_BASE`, `RP1_USB_BASE`
- [x] Software RTC (`kernel::rtc`): monotonic tick-based time tracking, AtomicU64 epoch, alarm support, datetime validation
- [x] Power management (`kernel::power`): CPU frequency get/set via VideoCore mailbox DVFS, min/max/voltage query, WFI idle
- [x] VideoCore mailbox DVFS extensions (`arch::aarch64::mailbox`): get/set clock rate, min/max clock, voltage query
- [x] ARMv8 Crypto Extensions driver (`kernel::crypto::hw`): AES-128/256 encrypt/decrypt (ECB, CBC, CTR) using hardware AESE/AESD/AESMC/AESIMC instructions, software key schedule (Rijndael), software SHA-256, runtime detection via ID_AA64ISAR0_EL1
- [x] Kernel peripheral manager extended (`kernel::periph`): PWM and Serial driver instances with cfg-gated RP1 drivers
- [x] 7 new syscalls: SYS_UART(15), SYS_PWM(16), SYS_RTC(17), SYS_DMA(18), SYS_USB(19), SYS_CRYPTO(20), SYS_POWER(21)
- [x] 7 capability bits: CAP_UART(1<<15) through CAP_POWER(1<<21), excluded from CAP_USER_DEFAULT
- [x] Syscall dispatch: 7 new dispatch functions with operation codes, user pointer validation, capability enforcement
- [x] SDR104 UHS-I mode (`arch::aarch64::emmc2`): 1.8V signaling, 208 MHz clock, CMD19 tuning, ADMA2 DMA transfers, graceful fallback to 25 MHz PIO
- [x] Shell commands: `pwm` (status), `rtc` (datetime/alarm), `power` (freq/voltage), `crypto` (detection), `uart` (port info)
- [x] Configuration constants in `os_cfg`: MAX_SERIAL_PORTS, PWM_CHANNELS, MAX_USB_DEVICES, CRYPTO_AES_BLOCK_SIZE, RTC_EPOCH_YEAR
- [x] Host-side tests: RTC datetime conversions (7 tests: epoch zero, roundtrip, leap year, known timestamp, boundary, days_in_month), plus existing 44 = 51 total
- [x] All 6 BSP×feature configurations build cleanly, all 51 host tests pass