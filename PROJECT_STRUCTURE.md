# Project Structure

```
tiny_os/
├── Cargo.toml              # Workspace root (members: kernel, arch, bsp, tests/host;
│                           #   default-members exclude tests/host from bare-metal builds)
├── Cargo.lock              # Locked dependency versions
├── rust-toolchain.toml     # Pins nightly channel + aarch64-unknown-none target
├── Makefile                # Build/test wrapper: make, make qemu, make img,
│                           #   make test, make test-host, make test-qemu
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
├── docs/                   # Specifications, API reference, and developer guides
│   ├── tiny_os_specification_v1.1.md   # System specification v1.2 — includes RTOS
│   │                                   #   certification: WCET, MC/DC, health monitor,
│   │                                   #   watchdog, mixed-criticality, traceability
│   ├── API_REFERENCE.md    # Complete API reference: shell, syscalls, scheduler,
│   │                       #   sync, filesystem, network, memory, HAL traits
│   └── USER_APP_GUIDE.md   # Developer's guide for user-space EL0 applications:
│                           #   syscall interface, static/dynamic deployment, constraints
│
├── examples/               # User-space applications (run at EL0 via syscalls)
│   └── temp_monitor.rs     # Temperature monitor: reads SoC temp via SYS_TEMPERATURE
│                           #   syscall, tracks min/max/avg, prints periodic status,
│                           #   all code in .user.text section (EL0-accessible)
│
├── arch/                   # Architecture crate — hardware register access & HAL traits
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # Crate root; re-exports arch-specific modules
│       ├── uart.rs         # UartDriver trait definition
│       ├── irq.rs          # InterruptController trait definition
│       ├── timer.rs        # Timer trait definition
│       ├── mm.rs           # PageAllocator trait definition
│       ├── context.rs      # Context HAL trait: new_context(), switch()
│       ├── smp.rs          # SmpBoot HAL trait: core_id(), num_cores(), start_core()
│       ├── block.rs        # BlockDevice HAL trait: sector read/write
│       ├── dma.rs          # DmaEngine HAL trait: channel-based DMA transfers
│       ├── net.rs          # NetDevice HAL trait: zero-copy packet TX/RX
│       ├── user.rs         # UserContext HAL trait: EL0 task isolation
│       └── aarch64/
│           ├── mod.rs      # AArch64 module root
│           ├── boot.S      # _start: DTB save, spin-table parking, secondary_boot
│           │               #   (EL3/EL2→EL1 drop for all cores), BSS zero, kmain
│           ├── vectors.S   # Exception vector table (2KB aligned, 16 entries),
│           │               #   TrapFrame save/restore macros, handler stubs
│           ├── exceptions.rs # TrapFrame struct, IRQ dispatch table, tick counter
│           ├── gic.rs      # GIC-400 (GICv2) driver: distributor + per-CPU interface,
│           │               #   SGI for IPI reschedule
│           ├── timer.rs    # ARM Generic Timer (virtual timer CNTV, 1kHz tick,
│           │               #   CVAL-based acknowledge, secondary core init)
│           ├── smp.rs      # AArch64 SMP: spin-table wakeup via SEV, core_id()
│           ├── mailbox.rs  # VideoCore mailbox driver: property tag interface,
│           │               #   SoC temperature query (tag 0x00030006)
│           ├── emmc2.rs    # SDHCI/EMMC2 SD card driver: PIO mode, card
│           │               #   init (CMD0/8/ACMD41/2/3/9/7), CSD parsing
│           ├── mmu.rs      # MMU setup: static L0/L1/L2 page tables, identity
│           │               #   mapping with 2MB blocks, W^X policy (RoCode RX,
│           │               #   Ram RW+NX, Device NX), MAIR/TCR/SCTLR config,
│           │               #   per-task TTBR0 (L3 4KB pages), ASID, switch_ttbr0
│           ├── context.rs  # Aarch64Context: builds fake stack frame for new
│           │               #   tasks, new_user_context for EL0, wraps context_switch FFI
│           └── context_switch.S  # AArch64 context switch (save/restore x19-x30,
│                           #   swap SP), task_trampoline (sched lock release +
│                           #   IRQ enable), task_trampoline_user (eret to EL0)
│
├── bsp/                    # Board Support Package crate — concrete HAL implementations
│   ├── Cargo.toml          # Features: bsp-rpi5 (default), bsp-qemu (mutually exclusive)
│   └── src/
│       ├── lib.rs          # Re-exports PlatformUart, GIC_DIST_BASE, GIC_CPU_BASE,
│       │                   #   MAILBOX_BASE based on active feature flag
│       ├── rpi5/
│       │   ├── mod.rs          # BSP root for Raspberry Pi 5
│       │   ├── memory_map.rs   # RP1_UART0_BASE = 0x1F_0006_C000 (36-bit PCIe window),
│       │   │                   #   GIC bases, MAILBOX_BASE, EMMC2_BASE, RAM default (4GB), peripheral + RP1 MMIO regions
│       │   └── rp1_uart.rs     # RP1 PL011 UART driver (MMIO volatile writes)
│       └── qemu_virt/
│           ├── mod.rs          # BSP root for QEMU raspi4b
│           ├── memory_map.rs   # UART at 0xFE20_1000, GIC bases, MAILBOX_BASE, EMMC2_BASE,
│           │                   #   RAM default (1GB), peripheral MMIO region
│           └── uart.rs         # BCM2711 PL011 UART driver
│
└── kernel/                 # Kernel binary crate
    ├── Cargo.toml          # Depends on arch + bsp; propagates bsp-* feature flags
    ├── link.ld             # Linker script: .text.boot at 0x80000, then .text,
    │                       #   .rodata, .user.text at 0x200000 (EL0 code),
    │                       #   ALIGN(2M) __data_start (W^X boundary), .data, .bss, .stack
    └── src/
        ├── main.rs         # kmain(): init UART/GIC/timer, mm, sched, net, user task,
        │                   #   SMP wakeup; secondary_main(): per-core MMU/GIC/timer init
        ├── panic.rs        # #[panic_handler]: print message + location, WFE halt
        ├── print.rs        # kprint!() / kprintln!() macros via spinlock-serialized Write
        ├── exceptions.rs   # IRQ dispatch (GIC acknowledge/EOI), IPI handler,
        │                   #   sync exception (SVC → syscall dispatch, EL0 fault handling),
        │                   #   unhandled trap
        ├── shell.rs        # Interactive UART shell: help, uptime, ticks, info, mem,
        │                   #   tasks, smp, log, health, sd, sdread, ls, cat, hexdump,
        │                   #   touch, write, ping, netstat, ifconfig, temp, exec [dynamic-load],
        │                   #   yield, svc, reboot
        ├── netbuf.rs       # Zero-copy DMA buffer pool: 1024×1536B in NC memory,
        │                   #   AtomicU8 refcount, spinlock-protected free list
        ├── syscall.rs      # Syscall dispatch: SYS_YIELD(0), SYS_DELAY(1), SYS_WRITE(2),
        │                   #   SYS_TASK_ID(3), SYS_UPTIME(4), SYS_EXIT(5), SYS_TEMPERATURE(6)
        ├── user_tasks.rs   # EL0 user demo task with inline-asm syscall stubs,
        │                   #   all code in .user.text section (EL0-accessible)
        ├── loader.rs       # [dynamic-load] ELF64 PIE loader: parse headers, load
        │                   #   PT_LOAD segments, apply relocations, create user tasks
        ├── net/            # Network stack subsystem
        │   ├── mod.rs      # Network init, RX dispatch loop, net_task, IP/MAC config
        │   ├── ethernet.rs # Ethernet frame parse/build (14-byte header, EtherType demux)
        │   ├── arp.rs      # ARP cache (16 entries), request/reply handling
        │   ├── ipv4.rs     # IPv4 parse/build (20-byte header), internet checksum
        │   ├── icmp.rs     # ICMP echo request/reply, ping RTT statistics
        │   ├── udp.rs      # UDP parse/build (8-byte header), port table (8 entries)
        │   ├── tcp.rs      # Minimal TCP: client SYN/ACK/FIN state machine (4 connections)
        │   ├── socket.rs   # BSD socket API: socket/bind/connect/sendto/recvfrom/close
        │   └── loopback.rs # Loopback NetDevice for QEMU (swaps src/dst, ICMP req→reply)
        ├── fs/             # Filesystem subsystem
        │   ├── mod.rs      # VFS layer: fd table (16 entries), open/read/write/close,
        │   │               #   readdir API, FsError enum, DirEntry type
        │   └── fat32.rs    # FAT32 driver: BPB parsing, FAT chain traversal, directory
        │                   #   entry parsing (8.3 + LFN), file read/write/create
        ├── storage/        # Storage subsystem
        │   ├── mod.rs      # ActiveDevice routing (EMMC2/RamDisk), static BlockCache,
        │   │               #   cached_read/write/flush, find_fat32_partition
        │   ├── ramdisk.rs  # 256KB in-memory FAT32 volume for QEMU testing
        │   ├── mbr.rs      # MBR partition table parser (4 entries, 0xAA55 sig)
        │   └── cache.rs    # LRU write-back block cache (32 lines × 512B)
        ├── spinlock.rs     # Ticket spinlock with IRQ save/restore for SMP
        ├── sched.rs        # SMP-aware 256-level fixed-priority scheduler: global run
        │                   #   queue with spinlock, per-core current task, per-core
        │                   #   idle tasks, IPI-triggered reschedule, budget enforcement,
        │                   #   criticality, stack watermarks, CPU utilization, TTBR0 swap,
        │                   #   task_create_user, task_terminate with DISCARD_SP
        ├── klog.rs         # Ring-buffer log subsystem: 5 levels (ERROR..TRACE),
        │                   #   timestamps, module tags, 64-entry buffer, BufWriter formatter
        ├── watchdog.rs     # Software watchdog: tick-based counter with configurable timeout,
        │                   #   auto-kick task at priority 0 ensures scheduler liveness
        ├── health.rs       # Health monitor task (priority 1): periodic stack watermark
        │                   #   scanning, CPU utilization checks, watchdog status, klog output
        ├── drivers.rs      # Driver trait (name/init/status) with 16-slot static registry
        ├── sync/           # Synchronization primitives subsystem
        │   ├── mod.rs      # WaitQueue: priority-sorted waiter array, lazy stale cleanup
        │   ├── mutex.rs    # Mutex with PIP/PCP, recursive locking (max depth 8), timeout
        │   ├── semaphore.rs # Counting/binary semaphore with timeout and try_wait
        │   ├── events.rs   # 32-bit event flags: Any/All wait modes, up to 16 waiters
        │   └── msgqueue.rs # Const-generic MsgQueue<MSG_SIZE, CAPACITY>: circular buffer,
        │                   #   separate send/recv wait queues, timeout + try variants
        └── mm/             # Memory management subsystem
            ├── mod.rs      # MM init: DTB RAM discovery → PMM → MMU enable → heap seed → DMA pool
            ├── dtb.rs      # Minimal FDT parser: extracts /memory node reg property
            ├── pmm.rs      # Bitmap page frame allocator: 1 bit per 4KB page, up to 4GB
            └── heap.rs     # Linked-list heap allocator: kmalloc/kfree, global stats

tests/                      # Two-tier test infrastructure
├── host/                   # Host-side unit tests (runs natively, not on bare-metal target)
│   ├── Cargo.toml          # Separate std crate; requires --target x86_64-pc-windows-msvc
│   └── src/
│       ├── lib.rs          # Crate root: declares ipv4, ethernet, mbr test modules
│       ├── ipv4.rs         # IPv4 checksum + header parsing (12 tests): RFC 1071,
│       │                   #   corruption detection, protocol parsing, edge cases
│       ├── ethernet.rs     # Ethernet frame parsing (7 tests): ethertype demux,
│       │                   #   header validation, broadcast detection
│       └── mbr.rs          # MBR partition table parsing (7 tests): FAT32/Linux/swap,
│                           #   signature validation, multi-partition, size calculation
└── qemu/
    └── run_tests.ps1       # QEMU integration test runner: builds kernel, boots on
                            #   raspi4b with 15s timeout, checks 13 serial output
                            #   patterns (banner, MMU, timer, scheduler, SMP×3,
                            #   network, filesystem, user mode, shell, no panic)
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
- SMP: all 4 cores start at `_start`; secondaries spin on `SMP_RELEASE_TABLE` (in `.data`)
  until core 0 writes the `secondary_boot` entry address and issues SEV. Each secondary
  does its own EL3→EL1 drop, per-core stack/GIC/MMU/timer init, then calls `secondary_main`.
- Run with `-smp 4` to enable all 4 cores.
