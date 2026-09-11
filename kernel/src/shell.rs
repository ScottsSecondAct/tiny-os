use crate::{audit, integrity, kprint, kprintln, fs, klog, mm, net, netbuf, os_cfg, periph, power, rtc, sched, storage, watchdog};
use arch::aarch64::{emmc2, exceptions, mailbox};
use arch::aarch64::mmu;
use arch::aarch64::smp;
use arch::aarch64::timer;
use arch::uart::UartDriver;

const BACKSPACE: u8 = 0x7F;

const AUTH_HASH: [u8; 32] = crate::crypto::sha256::const_hash(b"tiny_os");

fn authenticate(uart: &mut impl UartDriver) -> bool {
    if !os_cfg::SHELL_AUTH_EN {
        return true;
    }
    let mut attempts: u8 = 0;
    loop {
        kprint!("password: ");
        let mut pw = [0u8; 64];
        let mut pw_len: usize = 0;
        loop {
            if let Some(c) = try_read_byte(uart) {
                match c {
                    b'\r' | b'\n' => {
                        kprintln!();
                        break;
                    }
                    BACKSPACE | 0x08 => {
                        if pw_len > 0 {
                            pw_len -= 1;
                            kprint!("\x08 \x08");
                        }
                    }
                    0x20..=0x7E => {
                        if pw_len < pw.len() {
                            pw[pw_len] = c;
                            pw_len += 1;
                            kprint!("*");
                        }
                    }
                    _ => {}
                }
            } else {
                sched::delay(1);
            }
        }
        let hash = crate::crypto::sha256::hash(&pw[..pw_len]);
        if crate::crypto::hmac::constant_time_eq_pub(&hash, &AUTH_HASH) {
            audit::log(audit::AuditEvent::AuthOk, "shell");
            return true;
        }
        attempts += 1;
        audit::log(audit::AuditEvent::AuthFail, "shell");
        kprintln!("authentication failed ({}/{})", attempts, os_cfg::SHELL_AUTH_MAX_ATTEMPTS);
        if attempts >= os_cfg::SHELL_AUTH_MAX_ATTEMPTS {
            kprintln!("locked out for {}ms", os_cfg::SHELL_AUTH_LOCKOUT_MS);
            sched::delay(os_cfg::SHELL_AUTH_LOCKOUT_MS);
            attempts = 0;
        }
    }
}

pub fn run(uart: &mut impl UartDriver) -> ! {
    let mut buf = [0u8; 128];
    let mut len: usize = 0;

    authenticate(uart);

    kprintln!();
    kprint!("tiny_os> ");

    loop {
        let mut got_input = false;
        while let Some(c) = try_read_byte(uart) {
            got_input = true;
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

        if !got_input {
            sched::delay(1);
        }
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
            kprintln!("          smp, sd, sdread <lba>, ls [path], cat <path>,");
            kprintln!("          hexdump <path>, touch <path>, write <path> <text>,");
            kprintln!("          ping <ip>, netstat, ifconfig, temp,");
            kprintln!("          firewall, integrity, audit,");
            kprintln!("          pwm, rtc, power, crypto, uart,");
            kprintln!("          pac, security,");
            #[cfg(feature = "dynamic-load")]
            kprintln!("          exec <path>,");
            kprintln!("          faulttest, wcet, yield, svc, reboot");
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
        "ls" => {
            if !fs::is_mounted() {
                kprintln!("filesystem not mounted");
            } else {
                let path = if arg.is_empty() { "/" } else { arg };
                match fs::readdir_open(path) {
                    Ok(fd) => {
                        loop {
                            match fs::readdir_next(fd) {
                                Ok(Some(entry)) => {
                                    if entry.is_dir {
                                        kprintln!("  <DIR>  {}", entry.name_str());
                                    } else {
                                        kprintln!("  {:>5}  {}", entry.size, entry.name_str());
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    kprintln!("readdir error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        let _ = fs::readdir_close(fd);
                    }
                    Err(e) => kprintln!("ls: {:?}", e),
                }
            }
        }
        "cat" => {
            if !fs::is_mounted() {
                kprintln!("filesystem not mounted");
            } else if arg.is_empty() {
                kprintln!("usage: cat <path>");
            } else {
                match fs::open(arg, false) {
                    Ok(fd) => {
                        let mut buf = [0u8; 512];
                        loop {
                            match fs::read(fd, &mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    for &b in &buf[..n] {
                                        if b == 0 {
                                            break;
                                        }
                                        kprint!("{}", b as char);
                                    }
                                }
                                Err(e) => {
                                    kprintln!("\nread error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        let _ = fs::close(fd);
                    }
                    Err(e) => kprintln!("cat: {:?}", e),
                }
            }
        }
        "hexdump" if !arg.is_empty() => {
            if !fs::is_mounted() {
                kprintln!("filesystem not mounted");
            } else {
                match fs::open(arg, false) {
                    Ok(fd) => {
                        let mut buf = [0u8; 512];
                        loop {
                            match fs::read(fd, &mut buf) {
                                Ok(0) => break,
                                Ok(n) => hexdump(&buf[..n]),
                                Err(e) => {
                                    kprintln!("\nread error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        let _ = fs::close(fd);
                    }
                    Err(e) => kprintln!("hexdump: {:?}", e),
                }
            }
        }
        "touch" => {
            if !fs::is_mounted() {
                kprintln!("filesystem not mounted");
            } else if arg.is_empty() {
                kprintln!("usage: touch <path>");
            } else {
                match fs::create(arg) {
                    Ok(fd) => {
                        let _ = fs::close(fd);
                        kprintln!("created {}", arg);
                    }
                    Err(e) => kprintln!("touch: {:?}", e),
                }
            }
        }
        "write" => {
            if !fs::is_mounted() {
                kprintln!("filesystem not mounted");
            } else {
                // Split arg into "path text..."
                let (path, text) = match arg.find(' ') {
                    Some(i) => (&arg[..i], &arg[i+1..]),
                    None => {
                        kprintln!("usage: write <path> <text>");
                        ("", "")
                    }
                };
                if !path.is_empty() {
                    match fs::create(path) {
                        Ok(fd) => {
                            match fs::write(fd, text.as_bytes()) {
                                Ok(n) => kprintln!("wrote {} bytes to {}", n, path),
                                Err(e) => kprintln!("write error: {:?}", e),
                            }
                            let _ = fs::close(fd);
                        }
                        Err(e) => kprintln!("write: {:?}", e),
                    }
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
        "ping" => {
            if arg.is_empty() {
                kprintln!("usage: ping <ip>");
            } else {
                match parse_ipv4(arg) {
                    Some(ip) => {
                        let seq = net::icmp::send_ping(ip);
                        kprintln!("PING {} seq={}", ip, seq);
                        sched::delay(100);
                        match net::icmp::ping_result(seq) {
                            Some(rtt) => kprintln!("reply from {} seq={} time={}ms", ip, seq, rtt),
                            None => kprintln!("request timed out"),
                        }
                    }
                    None => kprintln!("invalid IP address"),
                }
            }
        }
        "netstat" => {
            kprintln!("IP:  {}", net::our_ip());
            kprintln!("MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                net::our_mac()[0], net::our_mac()[1], net::our_mac()[2],
                net::our_mac()[3], net::our_mac()[4], net::our_mac()[5]);
            kprintln!("ARP cache:");
            for entry in net::arp::cache_entries() {
                if entry.valid {
                    kprintln!("  {} -> {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                        entry.ip, entry.mac[0], entry.mac[1], entry.mac[2],
                        entry.mac[3], entry.mac[4], entry.mac[5]);
                }
            }
            kprintln!("sockets: {}", net::socket::socket_count());
        }
        "ifconfig" => {
            kprintln!("lo0: flags=UP,LOOPBACK mtu 1500");
            kprintln!("  inet {}", net::our_ip());
            kprintln!("  ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                net::our_mac()[0], net::our_mac()[1], net::our_mac()[2],
                net::our_mac()[3], net::our_mac()[4], net::our_mac()[5]);
        }
        "temp" => {
            match mailbox::get_temperature() {
                Some(mc) => kprintln!("{}.{}C", mc / 1000, ((mc % 1000).abs()) / 100),
                None => kprintln!("temperature unavailable"),
            }
        }
        #[cfg(feature = "dynamic-load")]
        "exec" => {
            if arg.is_empty() {
                kprintln!("usage: exec <path>");
            } else {
                match crate::loader::load_and_exec(arg) {
                    Ok(tid) => kprintln!("started task {}", tid),
                    Err(e) => kprintln!("exec failed: {:?}", e),
                }
            }
        }
        "firewall" => {
            use crate::net::firewall;
            let enabled = firewall::is_enabled();
            let count = firewall::rule_count();
            let (passed, dropped) = firewall::stats();
            kprintln!("firewall: {}", if enabled { "ENABLED (default-deny)" } else { "disabled" });
            kprintln!("rules:    {}/{}", count, os_cfg::MAX_FIREWALL_RULES);
            kprintln!("passed:   {}", passed);
            kprintln!("dropped:  {}", dropped);
            for i in 0..count {
                if let Some(rule) = firewall::get_rule(i) {
                    if rule.active {
                        kprintln!("  [{}] {}/{} port {} {}",
                            i, rule.src_ip, rule.src_mask, rule.dst_port, rule.protocol.as_str());
                    }
                }
            }
        }
        "integrity" => {
            let ok = integrity::verify();
            kprintln!("code integrity: {}", if ok { "OK" } else { "FAILED" });
            kprintln!("  boot CRC32:  {:#010x}", integrity::boot_crc());
            kprintln!("  .text size:  {} bytes", integrity::text_size());
            kprintln!("  checks run:  {}", integrity::check_count());
            kprintln!("  last check:  tick {}", integrity::last_check_tick());
        }
        "audit" => {
            if arg == "persist" {
                audit::persist_to_fs();
                kprintln!("audit log persisted to /audit.log");
            } else {
                let count = if arg.is_empty() { 20 } else { arg.parse::<usize>().unwrap_or(20) };
                audit::dump(count);
            }
        }
        "pwm" => {
            kprintln!("PWM: {} channels, 50 MHz reference", os_cfg::PWM_CHANNELS);
            kprintln!("  status: {}", if cfg!(feature = "bsp-rpi5") { "RP1 PWM available" } else { "not available (QEMU)" });
        }
        "rtc" => {
            match rtc::get_time() {
                Ok(dt) => kprintln!("RTC: {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second),
                Err(_) => kprintln!("RTC: unavailable"),
            }
            if rtc::check_alarm() {
                kprintln!("  alarm: TRIGGERED");
            } else {
                kprintln!("  alarm: not set");
            }
        }
        "power" => {
            match power::get_cpu_freq() {
                Ok(hz) => kprintln!("CPU freq: {} MHz", hz / 1_000_000),
                Err(_) => kprintln!("CPU freq: unavailable"),
            }
            match power::get_max_freq() {
                Ok(hz) => kprintln!("max freq: {} MHz", hz / 1_000_000),
                Err(_) => {}
            }
            match power::get_min_freq() {
                Ok(hz) => kprintln!("min freq: {} MHz", hz / 1_000_000),
                Err(_) => {}
            }
            match power::get_voltage() {
                Ok(uv) => kprintln!("voltage:  {} mV", uv / 1000),
                Err(_) => kprintln!("voltage:  unavailable"),
            }
        }
        "crypto" => {
            let supported = crate::crypto::hw::ArmCryptoEngine::detect();
            kprintln!("ARMv8 Crypto Extensions: {}", if supported { "available" } else { "not available" });
            kprintln!("  AES: ECB, CBC, CTR (128/256-bit keys)");
            kprintln!("  SHA-256: software (FIPS 180-4)");
            kprintln!("  CRC32: lookup table");
        }
        "uart" => {
            kprintln!("UART ports: {} max", os_cfg::MAX_SERIAL_PORTS);
            kprintln!("  port 0: console (PL011)");
            if cfg!(feature = "bsp-rpi5") {
                kprintln!("  ports 1-5: RP1 UART1-5 (48 MHz ref)");
            } else {
                kprintln!("  user ports: not available (QEMU)");
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
        "faulttest" => {
            crate::fault_inject::run_all();
        }
        "wcet" => {
            crate::wcet::dump_all();
        }
        "pac" => {
            crate::pac::status();
        }
        "security" => {
            kprintln!("--- Phase 14: Advanced Attack Hardening ---");
            kprintln!("PAC:         supported={}, active={}", crate::pac::is_supported(), crate::pac::is_active());
            kprintln!("Secure wipe: enabled={}", os_cfg::SECURE_WIPE_EN);
            kprintln!("Rate limit:  {}/{}ms", os_cfg::SYSCALL_RATE_LIMIT, os_cfg::SYSCALL_RATE_WINDOW_MS);
            kprintln!("--- Phase 13: Security Hardening ---");
            kprintln!("Auth:        enabled={}", os_cfg::SHELL_AUTH_EN);
            kprintln!("Debug lock:  enabled={}", os_cfg::DEBUG_LOCKDOWN);
            kprintln!("Firewall:    max_rules={}", os_cfg::MAX_FIREWALL_RULES);
        }
        "" => {}
        _ => {
            kprintln!("unknown command: {}", trimmed);
            kprintln!("type 'help' for available commands");
        }
    }
}

fn parse_ipv4(s: &str) -> Option<net::Ipv4Addr> {
    let mut octets = [0u8; 4];
    let mut parts = s.split('.');
    for octet in octets.iter_mut() {
        let part = parts.next()?;
        *octet = part.parse::<u8>().ok()?;
    }
    if parts.next().is_some() { return None; }
    Some(net::Ipv4Addr(octets))
}

fn parse_u64(s: &str) -> u64 {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).unwrap_or(0)
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

fn hexdump(data: &[u8]) {
    let rows = (data.len() + 15) / 16;
    for row in 0..rows {
        let off = row * 16;
        let count = (data.len() - off).min(16);
        kprint!("{:04x}: ", off);
        for i in 0..16 {
            if i < count {
                kprint!("{:02x} ", data[off + i]);
            } else {
                kprint!("   ");
            }
            if i == 7 {
                kprint!(" ");
            }
        }
        kprint!(" |");
        for i in 0..count {
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
