# tiny_os User-Space Application Developer's Guide

This guide covers everything you need to write, build, and run applications on tiny_os. User-space apps execute at AArch64 Exception Level 0 (EL0) with memory isolation enforced by per-task page tables. All hardware access is mediated through kernel syscalls.

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Deployment Models](#deployment-models)
- [Syscall Reference](#syscall-reference)
- [Writing a Static Application](#writing-a-static-application)
- [Writing a Dynamically Loaded Application](#writing-a-dynamically-loaded-application)
- [Programming Constraints](#programming-constraints)
- [String and Data Handling](#string-and-data-handling)
- [Formatting Output](#formatting-output)
- [Debugging](#debugging)
- [Examples](#examples)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  User Application (EL0)                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────────┐  │
│  │ App Code │  │ App Data │  │ User Stack (16 KB)       │  │
│  │ (RX)     │  │ (RW)     │  │ (RW, guard page below)  │  │
│  └──────────┘  └──────────┘  └──────────────────────────┘  │
│                      │ SVC #0                               │
├──────────────────────┼──────────────────────────────────────┤
│  Kernel (EL1)        ▼                                      │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ Syscall Dispatch (X8=nr, X0-X1=args, X0=return)     │   │
│  │  → yield, delay, write, task_id, uptime, exit, temp │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

Each user task gets:

- **Its own TTBR0 page table** — cloned from the kernel's, with EL0-accessible regions added for app code, data, and stack.
- **An 8-bit ASID** — TLB entries are tagged so context switches between user tasks don't require a full TLB flush.
- **A kernel stack** — used when handling syscalls and exceptions on behalf of the task.
- **A user stack** — 16 KB (static) or 16 KB (dynamic loader), with a 4 KB unmapped guard page below to catch stack overflow.

Memory protection is enforced by the MMU with W^X policy:

| Region | Permissions | Description |
|--------|-------------|-------------|
| Code | EL0 Read + Execute | Application instructions |
| Data/Stack | EL0 Read + Write, No Execute | Variables, buffers, stack |
| Kernel | EL1 only | Inaccessible from EL0 — faults if touched |

---

## Deployment Models

tiny_os supports two ways to run user-space applications:

### 1. Static Linking (Default — Production)

The application is compiled into the kernel image and placed in the `.user.text` linker section. The kernel creates page tables that make this region EL0-accessible at boot.

**Advantages:** Deterministic, no filesystem dependency, suitable for safety certification (IEC 61508, ISO 26262, DO-178C).

**How it works:**
1. Write your app as a Rust module in `examples/` or `kernel/src/`.
2. Annotate all functions and static data with `#[link_section = ".user.text"]`.
3. Add a `#[path]` module reference in `kernel/src/main.rs`.
4. Create the user task in `kmain()` with `task_create_user()`.

### 2. Dynamic Loading (Optional — Development)

The application is compiled as a standalone ELF64 PIE binary, placed on the FAT32 filesystem, and loaded at runtime via the `exec` shell command.

**Advantages:** Iterate on apps without rebuilding the kernel. Load different apps without reflashing.

**Requires:** The `dynamic-load` Cargo feature flag (disabled by default).

```sh
# Build kernel with dynamic loading enabled
cargo build --features kernel/dynamic-load
```

**How it works:**
1. Write your app as a standalone `#![no_std]` `#![no_main]` crate.
2. Cross-compile as a PIE shared object targeting `aarch64-unknown-none`.
3. Place the `.elf` file on the FAT32 filesystem (SD card or ramdisk).
4. Run `exec /path/to/app.elf` at the shell prompt.

---

## Syscall Reference

Syscalls are invoked via `SVC #0`. The syscall number goes in register **X8**, arguments in **X0** and **X1**, and the return value comes back in **X0**.

| # | Name | X0 (arg/return) | X1 (arg) | Description |
|---|------|-----------------|----------|-------------|
| 0 | `SYS_YIELD` | — / 0 | — | Yield the current timeslice to other tasks |
| 1 | `SYS_DELAY` | milliseconds / 0 | — | Sleep for the specified duration |
| 2 | `SYS_WRITE` | buffer pointer / bytes written | length (max 256) | Write a UTF-8 string to the kernel console |
| 3 | `SYS_TASK_ID` | — / task ID | — | Get the current task's numeric ID |
| 4 | `SYS_UPTIME` | — / ticks | — | Get system uptime in milliseconds |
| 5 | `SYS_EXIT` | — / (no return) | — | Terminate the current task |
| 6 | `SYS_TEMPERATURE` | — / millidegrees C | — | Read SoC temperature (u64::MAX if unavailable) |

### Syscall Stub Implementation

Every user-space application needs a syscall stub. This is the fundamental building block — a single inline assembly function that all syscall wrappers call:

```rust
use core::arch::asm;

#[inline(always)]
fn syscall(nr: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inout("x0") a0 => ret,
            in("x1") a1,
            lateout("x2") _,
            lateout("x3") _,
        );
    }
    ret
}
```

Then build typed wrappers on top:

```rust
#[inline(always)]
fn sys_yield() { syscall(0, 0, 0); }

#[inline(always)]
fn sys_delay(ms: u32) { syscall(1, ms as u64, 0); }

#[inline(always)]
fn sys_write(ptr: *const u8, len: usize) { syscall(2, ptr as u64, len as u64); }

#[inline(always)]
fn sys_task_id() -> u64 { syscall(3, 0, 0) }

#[inline(always)]
fn sys_uptime() -> u64 { syscall(4, 0, 0) }

#[inline(always)]
fn sys_exit() -> ! {
    syscall(5, 0, 0);
    loop { unsafe { asm!("wfe"); } }
}

#[inline(always)]
fn sys_temperature() -> i32 { syscall(6, 0, 0) as i32 }
```

---

## Writing a Static Application

Static applications live inside the kernel binary image. Here's the complete process:

### Step 1: Create the Application File

Create `examples/my_app.rs`:

```rust
//! My Application — user-space app for tiny_os
//!
//! Runs at EL0. All hardware access via syscalls.

use core::arch::asm;

// ── Syscall interface ──────────────────────────────────────────

#[inline(always)]
fn syscall(nr: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inout("x0") a0 => ret,
            in("x1") a1,
            lateout("x2") _,
            lateout("x3") _,
        );
    }
    ret
}

#[inline(always)]
fn sys_delay(ms: u32) { syscall(1, ms as u64, 0); }

#[inline(always)]
fn sys_write(ptr: *const u8, len: usize) { syscall(2, ptr as u64, len as u64); }

#[inline(always)]
fn sys_exit() -> ! {
    syscall(5, 0, 0);
    loop { unsafe { asm!("wfe"); } }
}

// ── String constants (MUST be in .user.text) ───────────────────

#[link_section = ".user.text"]
static MSG_HELLO: [u8; 20] = *b"[my-app] hello EL0!\n";

#[link_section = ".user.text"]
static MSG_DONE: [u8; 16] = *b"[my-app] done.\n";

// ── Entry point ────────────────────────────────────────────────

#[link_section = ".user.text"]
pub fn my_app_main(_arg: usize) -> ! {
    sys_write(MSG_HELLO.as_ptr(), 20);

    for _ in 0..5 {
        sys_delay(1000);
        sys_write(MSG_HELLO.as_ptr(), 20);
    }

    sys_write(MSG_DONE.as_ptr(), 15);
    sys_exit();
}
```

### Step 2: Register the Module

In `kernel/src/main.rs`, add a path-based module declaration alongside the existing ones:

```rust
#[path = "../../examples/my_app.rs"]
mod my_app;
```

### Step 3: Create the User Task

In the `kmain()` function, after the existing user task creation code, add:

```rust
// Allocate stacks (static, so they live for the lifetime of the kernel).
#[repr(align(4096))]
struct MyAppUserStack([u8; 16384]);
static mut MY_APP_USER_STACK: MyAppUserStack = MyAppUserStack([0; 16384]);

struct MyAppKernelStack([u8; 8192]);
static mut MY_APP_KERNEL_STACK: MyAppKernelStack = MyAppKernelStack([0; 8192]);

// Set up the task.
let entry = my_app::my_app_main as *const () as usize;
let stack_base = unsafe { &raw const MY_APP_USER_STACK.0 as usize };
let stack_top = stack_base + 16384;

let ttbr0 = unsafe {
    mmu::create_user_page_table(code_base, code_size, stack_base, 16384 / 4096)
};

let kernel_stack = unsafe { &mut MY_APP_KERNEL_STACK.0[..] };
sched::task_create_user(
    "my-app", 100, Criticality::Standard,
    kernel_stack, entry, stack_top, 0, ttbr0,
).expect("failed to create my-app task");
```

Note: `code_base` and `code_size` are already computed from the `__user_text_start` / `__user_text_end` linker symbols — your app's `.user.text` code is part of that same region.

### Step 4: Build and Test

```sh
cargo build --no-default-features --features kernel/bsp-qemu
```

---

## Writing a Dynamically Loaded Application

Dynamically loaded apps are standalone ELF64 PIE binaries. They are fully self-contained — no kernel module registration needed.

### Requirements

- **Format:** ELF64, AArch64, Position-Independent Executable (`ET_DYN`)
- **Relocations:** Only `R_AARCH64_RELATIVE` (no symbol lookups)
- **Segments:** Up to 8 PT_LOAD segments
- **Entry point:** A `_start` function (or whatever `e_entry` points to)
- **Stack:** 16 KB allocated by the loader (not provided by the app)
- **No standard library** — `#![no_std]`, `#![no_main]`

### Minimal Example

```rust
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[inline(always)]
fn syscall(nr: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inout("x0") a0 => ret,
            in("x1") a1,
            lateout("x2") _,
            lateout("x3") _,
        );
    }
    ret
}

fn sys_write(ptr: *const u8, len: usize) { syscall(2, ptr as u64, len as u64); }
fn sys_delay(ms: u32) { syscall(1, ms as u64, 0); }

fn sys_exit() -> ! {
    syscall(5, 0, 0);
    loop { unsafe { asm!("wfe"); } }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"Hello from dynamically loaded app!\n";
    sys_write(msg.as_ptr(), msg.len());

    for _ in 0..10 {
        sys_delay(1000);
        let tick = b"[app] tick\n";
        sys_write(tick.as_ptr(), tick.len());
    }

    sys_exit();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { unsafe { asm!("wfe"); } }
}
```

### Build Configuration

Create a `Cargo.toml` for the app:

```toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2021"

[profile.release]
panic = "abort"
opt-level = "s"
lto = true
```

Create a `.cargo/config.toml`:

```toml
[build]
target = "aarch64-unknown-none"

[target.aarch64-unknown-none]
rustflags = ["-C", "relocation-model=pic"]
```

Build as a PIE shared library (which produces a relocatable ELF):

```sh
cargo build --release
```

The linker will produce an ELF64 binary. Verify the format:

```sh
aarch64-linux-gnu-readelf -h target/aarch64-unknown-none/release/my-app
# Type should show "DYN (Position-Independent Executable)"
# Machine should show "AArch64"
```

### Deploy and Run

1. Copy the ELF to the FAT32 filesystem (SD card or ramdisk).
2. Boot the kernel with `dynamic-load` enabled.
3. At the shell prompt:

```
tiny_os> exec /my-app
loader: loaded '/my-app' at 0x2400000, entry 0x2400080, task 8
started task 8
Hello from dynamically loaded app!
[app] tick
[app] tick
...
```

---

## Programming Constraints

User-space apps run in a restricted environment. These constraints apply to both static and dynamic apps:

### No Standard Library

There is no `std`, no `libc`, no heap allocator. You have:

- `core` — Rust's freestanding core library (types, traits, iterators, etc.)
- Syscalls — the only way to interact with the outside world

### No Global Allocator

`alloc` is not available. No `Vec`, `String`, `Box`, or `HashMap`. Use:

- Fixed-size arrays on the stack
- `MaybeUninit` for large uninitialized stack buffers
- `core::mem::zeroed()` for zero-initialized arrays

### No Floating Point

The kernel does not save/restore FP/SIMD registers across context switches for user tasks. Avoid `f32`/`f64` operations.

### No Panicking with Formatting

`panic!("message {}", val)` pulls in formatting machinery that may generate code the linker can't resolve (e.g., `memcpy`, overflow checks). For static apps, panics in `.user.text` code should be avoided entirely. For dynamic apps, provide a minimal panic handler:

```rust
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { unsafe { core::arch::asm!("wfe"); } }
}
```

### SYS_WRITE Buffer Limit

`SYS_WRITE` accepts at most 256 bytes per call. Longer messages must be split across multiple calls.

### Stack Size

The user stack is 16 KB (4 pages). Deep recursion or large stack allocations will overflow into the guard page and trigger a Data Abort, terminating the task. Keep stack usage shallow and prefer static/global buffers where possible.

---

## String and Data Handling

### Static Apps: The `.user.text` Rule

In static apps, **all string literals and static data must be placed in the `.user.text` section**. The default `.rodata` section is mapped as EL1-only — accessing it from EL0 causes a Data Abort.

```rust
// WRONG — this will fault at EL0:
static MSG: &[u8] = b"hello\n";

// CORRECT — accessible from EL0:
#[link_section = ".user.text"]
static MSG: [u8; 6] = *b"hello\n";
```

Note the pattern: use a fixed-size byte array (`[u8; N]`), not a slice (`&[u8]`) or `&str`. Slice metadata (pointer + length) would be stored in `.rodata`, causing a fault when the runtime tries to read it.

### Dynamic Apps: No Section Annotations Needed

Dynamically loaded apps don't need `#[link_section]` annotations. The loader maps all segments with appropriate EL0 permissions — `.rodata` is loaded alongside `.text` as part of the ELF image.

```rust
// This works fine in a dynamically loaded app:
let msg = b"hello from dynamic app\n";
sys_write(msg.as_ptr(), msg.len());
```

### Avoiding memcpy

The compiler may silently emit calls to `memcpy`, `memset`, or `__aeabi_memcpy` for array copies, struct initialization, or large stack moves. These symbols don't exist in the bare-metal environment.

Use `core::ptr::write_volatile` / `read_volatile` for byte-by-byte operations:

```rust
// Instead of: buf[i] = value;
unsafe { core::ptr::write_volatile(buf.as_mut_ptr().add(i), value); }

// Instead of: let x = buf[i];
let x = unsafe { core::ptr::read_volatile(buf.as_ptr().add(i)) };
```

---

## Formatting Output

There is no `format!()` or `write!()` in user space. You must format output manually. Here's a reusable pattern for writing decimal numbers:

```rust
#[inline(always)]
fn write_u32(mut n: u32) {
    let mut buf = [0u8; 11]; // max 10 digits + newline
    let ptr = buf.as_mut_ptr();

    if n == 0 {
        unsafe {
            core::ptr::write_volatile(ptr, b'0');
            core::ptr::write_volatile(ptr.add(1), b'\n');
        }
        sys_write(ptr, 2);
        return;
    }

    // Extract digits in reverse.
    let mut tmp = [0u8; 10];
    let tp = tmp.as_mut_ptr();
    let mut len: usize = 0;
    while n > 0 {
        unsafe {
            core::ptr::write_volatile(tp.add(len), b'0' + (n % 10) as u8);
        }
        n /= 10;
        len += 1;
    }

    // Reverse into output buffer.
    let mut pos: usize = 0;
    let mut i = len;
    while i > 0 {
        i -= 1;
        unsafe {
            let c = core::ptr::read_volatile(tp.add(i));
            core::ptr::write_volatile(ptr.add(pos), c);
        }
        pos += 1;
    }
    unsafe { core::ptr::write_volatile(ptr.add(pos), b'\n'); }
    pos += 1;

    sys_write(ptr, pos);
}
```

For building multi-field output lines, use a `MaybeUninit` buffer and track the write position:

```rust
let mut buf: core::mem::MaybeUninit<[u8; 128]> = core::mem::MaybeUninit::uninit();
let p = buf.as_mut_ptr() as *mut u8;
let mut pos: usize = 0;

// Append a static label.
let label = b"count: ";
let mut i = 0;
while i < label.len() {
    unsafe { core::ptr::write_volatile(p.add(pos + i), label[i]); }
    i += 1;
}
pos += label.len();

// Append the formatted number, newline, then write.
// ... (use the digit extraction pattern above)

sys_write(p, pos);
```

---

## Debugging

### Task Status

Check your task is running with the `tasks` shell command:

```
tiny_os> tasks
 ID  Name        Pri  State    Crit        Ticks
  0  idle-0      255  Ready    Standard        0
  1  shell        10  Running  Standard    12045
  ...
  8  elf-app     100  Ready    Standard      340
```

### Console Output

All `SYS_WRITE` output goes to the kernel's UART console. Prefix messages with your app name to distinguish output from other tasks:

```rust
#[link_section = ".user.text"]
static PREFIX: [u8; 9] = *b"[my-app] ";
```

### Fault Diagnosis

If your task crashes, the kernel prints a diagnostic:

```
[fault] data abort from EL0: task 8
  ESR: 0x96000004  FAR: 0x00000000DEADBEEF
  ELR: 0x00200104  SPSR: 0x00000000
  X0:  0x...  X1: 0x...  ...
```

Common causes:

| Fault | Likely Cause |
|-------|-------------|
| FAR in kernel range (>= `__data_start`) | Accessing kernel memory from EL0 |
| FAR = 0 or small value | Null pointer dereference |
| FAR just below stack base | Stack overflow (hit guard page) |
| ELR in `.rodata` range | Static app reading string not in `.user.text` |
| Permission fault on code page | Writing to a read-only code page |

### SYS_UPTIME for Timing

Use `SYS_UPTIME` to measure elapsed time (returns milliseconds):

```rust
let start = sys_uptime();
// ... do work ...
let elapsed = sys_uptime() - start;
```

---

## Examples

### examples/temp_monitor.rs (Static)

The temperature monitor is a complete production-quality static app. It demonstrates:

- Periodic polling with `SYS_DELAY`
- Hardware access via `SYS_TEMPERATURE`
- Statistics tracking (min/max/running average)
- Formatted multi-field output with `MaybeUninit` buffers
- Volatile read/write to prevent compiler-generated `memcpy`
- All data in `.user.text` section

### kernel/src/user_tasks.rs (Static)

The user demo task is a minimal static app showing:

- Syscall stub pattern
- String constants in `.user.text`
- Startup message with task ID
- Periodic counter with formatted decimal output

---

## Quick Reference Card

| What | Static App | Dynamic App |
|------|-----------|-------------|
| File location | `examples/` or `kernel/src/` | Separate crate |
| Registration | Module in `main.rs` + `task_create_user()` | `exec` shell command |
| String data | `#[link_section = ".user.text"]` required | No annotation needed |
| Entry signature | `pub fn name(_arg: usize) -> !` | `pub extern "C" fn _start() -> !` |
| Panic handler | Shared with kernel | Must provide own `#[panic_handler]` |
| Build feature | None (always available) | `dynamic-load` Cargo feature |
| Certification | Suitable (static linking) | Development only |
| Stack | 16 KB, statically allocated | 16 KB, dynamically allocated |
| Binary format | Part of `kernel8.img` | Standalone ELF64 PIE |
