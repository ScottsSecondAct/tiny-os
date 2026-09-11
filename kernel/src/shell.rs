use crate::{kprint, kprintln, klog, mm, netbuf, sched, storage, watchdog};
use arch::aarch64::{emmc2, exceptions};
use arch::aarch64::mmu;
use arch::aarch64::smp;
use arch::aarch64::timer;
use arch::uart::UartDriver;

const BACKSPACE: u8 = 0x7F;

pub fn run(uart: &mut impl UartDriver) -> ! {
    let mut buf = [0u8; 64];
    let mut len: usize = 0;

    kprintln!();
    kprint!("tiny_os> ");

    loop {
        if let Some(c) = try_read_byte(uart) {
            match c {
                b'\r' | b'\n' => {
                    kprintln!();
                    if len > 0 {
                        let cmd = core::str::from_utf8(&buf[..len]).unwrap_or("");
                        dispatch(cmd);
                        len = 0;
                    }
                    kprint!("tiny_os> ");
                }
                BACKSPACE | 0x08 => {
                    if len > 0 {
                        len -= 1;
                        kprint!("\x08 \x08");
                    }
                }
                0x20..=0x7E => {
                    if len < buf.len() {
                        buf[len] = c;
                        len += 1;
                        uart.write_byte(c);
                    }
                }
                _ => {}
            }
        }

        // No input available — sleep briefly so lower-priority tasks can run.
        sched::delay(1);
    }
}

fn try_read_byte(_uart: &mut impl UartDriver) -> Option<u8> {
    let fr_addr = uart_fr_addr();
    if fr_addr == 0 {
        return None;
    }
    // SAFETY: fr_addr is a valid MMIO register for the active BSP UART.
    let fr = unsafe { core::ptr::read_volatile(fr_addr as *const u32) };
    if fr & (1 << 4) != 0 {
        return None; // RX FIFO empty
    }
    let dr_addr = fr_addr - 0x18; // DR is at offset 0, FR at 0x18
    // SAFETY: Same MMIO region.
    let data = unsafe { core::ptr::read_volatile(dr_addr as *const u32) };
    Some(data as u8)
}

fn uart_fr_addr() -> usize {
    #[cfg(feature = "bsp-rpi5")]
    {
        bsp::rpi5::memory_map::RP1_UART0_BASE + 0x18
    }
    #[cfg(feature = "bsp-qemu")]
    {
        bsp::qemu_virt::memory_map::UART0_BASE + 0x18
    }
}

fn dispatch(cmd: &str) {
    let trimmed = cmd.trim();
    let (base, arg) = match trimmed.find(' ') {
        Some(i) => (&trimmed[..i], trimmed[i+1..].trim()),
        None => (trimmed, ""),
    };

    match base {
        "help" => {
            kprintln!("commands: help, uptime, ticks, info, mem, tasks, log, health,");
            kprintln!("          smp, sd, sdread <lba>, yield, svc, reboot");
        }
        "uptime" => {
            let ticks = exceptions::tick_count();
            let ms = ticks;
            let secs = ms / 1000;
            let frac = ms % 1000;
            kprintln!("uptime: {}.{:03}s ({} ticks)", secs, frac, ticks);
        }
        "ticks" => {
            kprintln!("{}", exceptions::tick_count());
        }
        "info" => {
            let freq = timer::frequency();
            kprintln!("timer freq:  {} Hz", freq);
            kprintln!("tick rate:   1000 Hz");
            kprintln!("tick count:  {}", exceptions::tick_count());
            kprintln!("cores:       {}", sched::active_cores());
            kprintln!("this core:   {}", smp::core_id());
        }
        "mem" => {
            let (total, used, free) = mm::page_stats();
            kprintln!("pages:  {} total, {} used, {} free ({} KB free)", total, used, free, free * 4);
            let (htotal, hused, hfree) = mm::heap_stats();
            kprintln!("heap:   {} total, {} used, {} free", htotal, hused, hfree);
            let (nb_total, nb_free) = netbuf::pool_stats();
            kprintln!("netbuf: {} total, {} free", nb_total, nb_free);
            kprintln!("MMU:    {}", if mmu::enabled() { "on" } else { "off" });
        }
        "tasks" => {
            kprintln!("{:<4} {:<12} {:<6} {:<10} {:<8} {:<8}", "ID", "NAME", "PRIO", "STATE", "CRIT", "CPU");
            for entry in sched::task_list_ext().iter() {
                let (id, name, prio, state, crit, _budget, run_ticks) = *entry;
                if state != sched::TaskState::Dormant {
                    let state_str = match state {
                        sched::TaskState::Ready => "ready",
                        sched::TaskState::Running => "running",
                        sched::TaskState::Blocked => "blocked",
                        sched::TaskState::Suspended => "suspend",
                        sched::TaskState::Dormant => "dormant",
                    };
                    kprintln!("{:<4} {:<12} {:<6} {:<10} {:<8} {:<8}",
                        id, name, prio, state_str, crit.as_str(), run_ticks);
                }
            }
        }
        "smp" => {
            let cores = sched::active_cores();
            kprintln!("SMP: {} cores active", cores);
            kprintln!("this core: {}", smp::core_id());
        }
        "sd" => {
            if !emmc2::is_initialized() {
                kprintln!("SD card not initialized");
            } else if let Some((sdhc, blocks)) = emmc2::card_info() {
                kprintln!("type:       {}", if sdhc { "SDHC" } else { "SDSC" });
                kprintln!("blocks:     {}", blocks);
                kprintln!("capacity:   {} MB", blocks / 2048);
                kprintln!("block size: 512");
                storage::print_mbr_info();
            }
        }
        "sdread" => {
            if !emmc2::is_initialized() {
                kprintln!("SD card not initialized");
            } else if arg.is_empty() {
                kprintln!("usage: sdread <lba>");
            } else {
                let lba = parse_u64(arg);
                let mut buf = [0u8; 512];
                match emmc2::read_block(lba, &mut buf) {
                    Ok(()) => {
                        kprintln!("sector {} (LBA {:#x}):", lba, lba);
                        hexdump(&buf);
                    }
                    Err(e) => kprintln!("read error: {:?}", e),
                }
            }
        }
        "log" => {
            if arg.starts_with("level ") {
                let level_str = arg[6..].trim();
                match klog::LogLevel::from_str(level_str) {
                    Some(level) => {
                        klog::set_level(level);
                        kprintln!("log level set to {}", level.as_str());
                    }
                    None => kprintln!("unknown level: {} (use error/warn/info/debug/trace)", level_str),
                }
            } else {
                let count = if arg.is_empty() {
                    16
                } else {
                    arg.parse::<usize>().unwrap_or(16)
                };
                klog::dump(count);
            }
        }
        "health" => {
            kprintln!("Stack watermarks:");
            for &(_id, name, used, size) in sched::task_stack_info().iter() {
                if size == 0 {
                    continue;
                }
                let pct = (used * 100) / size;
                kprintln!("  {:<12}: {}/{} ({}%)", name, used, size, pct);
            }
            let (busy, total) = sched::utilization();
            if total > 0 {
                let cpu_pct = (busy * 100) / total;
                kprintln!("CPU utilization: {}% (idle {}%)", cpu_pct, 100 - cpu_pct);
            }
            if watchdog::is_enabled() {
                kprintln!("Watchdog: ok ({}ms timeout, counter {}ms)", watchdog::timeout(), watchdog::counter());
            } else {
                kprintln!("Watchdog: disabled");
            }
        }
        "yield" => {
            kprintln!("yielding...");
            sched::task_yield();
        }
        "svc" => {
            unsafe { core::arch::asm!("svc #42") };
        }
        "reboot" => {
            kprintln!("rebooting...");
            unsafe { core::arch::asm!("msr daifset, #15") };
            loop {
                unsafe { core::arch::asm!("wfe") };
            }
        }
        "" => {}
        _ => {
            kprintln!("unknown command: {}", trimmed);
            kprintln!("type 'help' for available commands");
        }
    }
}

fn parse_u64(s: &str) -> u64 {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

fn hexdump(data: &[u8]) {
    for row in 0..(data.len() / 16) {
        let off = row * 16;
        kprint!("{:04x}: ", off);
        for i in 0..16 {
            kprint!("{:02x} ", data[off + i]);
            if i == 7 {
                kprint!(" ");
            }
        }
        kprint!(" |");
        for i in 0..16 {
            let b = data[off + i];
            if (0x20..=0x7E).contains(&b) {
                kprint!("{}", b as char);
            } else {
                kprint!(".");
            }
        }
        kprintln!("|");
    }
}
