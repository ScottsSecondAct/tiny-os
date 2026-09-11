use crate::os_cfg;
use crate::spinlock::SpinLock;
use core::sync::atomic::{AtomicU64, Ordering};
use super::ipv4::{self, Ipv4Header, PROTO_ICMP, PROTO_TCP, PROTO_UDP};
use super::Ipv4Addr;

const MAX_RULES: usize = os_cfg::MAX_FIREWALL_RULES;

#[derive(Clone, Copy, PartialEq)]
pub enum Protocol {
    Any,
    Icmp,
    Udp,
    Tcp,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Any => "any",
            Protocol::Icmp => "ICMP",
            Protocol::Udp => "UDP",
            Protocol::Tcp => "TCP",
        }
    }

    fn matches(self, proto_byte: u8) -> bool {
        match self {
            Protocol::Any => true,
            Protocol::Icmp => proto_byte == PROTO_ICMP,
            Protocol::Udp => proto_byte == PROTO_UDP,
            Protocol::Tcp => proto_byte == PROTO_TCP,
        }
    }
}

#[derive(Clone, Copy)]
pub struct FirewallRule {
    pub src_ip: Ipv4Addr,
    pub src_mask: Ipv4Addr,
    pub dst_port: u16,
    pub protocol: Protocol,
    pub active: bool,
}

impl FirewallRule {
    const fn empty() -> Self {
        Self {
            src_ip: Ipv4Addr::ZERO,
            src_mask: Ipv4Addr::ZERO,
            dst_port: 0,
            protocol: Protocol::Any,
            active: false,
        }
    }
}

static LOCK: SpinLock = SpinLock::new();
static mut RULES: [FirewallRule; MAX_RULES] = [FirewallRule::empty(); MAX_RULES];
static mut RULE_COUNT: usize = 0;
static mut ENABLED: bool = false;

static PASSED: AtomicU64 = AtomicU64::new(0);
static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let saved = LOCK.lock();
    unsafe {
        RULE_COUNT = 0;
        ENABLED = false;
        for r in RULES.iter_mut() {
            *r = FirewallRule::empty();
        }
    }
    LOCK.unlock(saved);
}

pub fn enable() {
    let saved = LOCK.lock();
    unsafe { ENABLED = true };
    LOCK.unlock(saved);
}

pub fn disable() {
    let saved = LOCK.lock();
    unsafe { ENABLED = false };
    LOCK.unlock(saved);
}

pub fn is_enabled() -> bool {
    let saved = LOCK.lock();
    let e = unsafe { ENABLED };
    LOCK.unlock(saved);
    e
}

pub fn add_rule(src_ip: Ipv4Addr, src_mask: Ipv4Addr, dst_port: u16, protocol: Protocol) -> bool {
    let saved = LOCK.lock();
    let count = unsafe { RULE_COUNT };
    if count >= MAX_RULES {
        LOCK.unlock(saved);
        return false;
    }
    unsafe {
        RULES[count] = FirewallRule {
            src_ip,
            src_mask,
            dst_port,
            protocol,
            active: true,
        };
        RULE_COUNT = count + 1;
    }
    LOCK.unlock(saved);
    true
}

pub fn clear_rules() {
    let saved = LOCK.lock();
    unsafe {
        RULE_COUNT = 0;
        for r in RULES.iter_mut() {
            *r = FirewallRule::empty();
        }
    }
    LOCK.unlock(saved);
}

fn ip_matches(addr: Ipv4Addr, rule_ip: Ipv4Addr, mask: Ipv4Addr) -> bool {
    let a = addr.as_u32();
    let r = rule_ip.as_u32();
    let m = mask.as_u32();
    (a & m) == (r & m)
}

fn extract_dst_port(ip_data: &[u8], hdr: &Ipv4Header) -> u16 {
    let transport = &ip_data[hdr.header_len..];
    if transport.len() < 4 {
        return 0;
    }
    u16::from_be_bytes([transport[2], transport[3]])
}

pub fn check_packet(ip_data: &[u8], hdr: &Ipv4Header) -> bool {
    let saved = LOCK.lock();
    let enabled = unsafe { ENABLED };
    if !enabled {
        LOCK.unlock(saved);
        PASSED.fetch_add(1, Ordering::Relaxed);
        return true;
    }

    let count = unsafe { RULE_COUNT };
    let dst_port = extract_dst_port(ip_data, hdr);

    for i in 0..count {
        let rule = unsafe { &RULES[i] };
        if !rule.active {
            continue;
        }
        if !rule.protocol.matches(hdr.protocol) {
            continue;
        }
        if rule.src_mask != Ipv4Addr::ZERO && !ip_matches(hdr.src, rule.src_ip, rule.src_mask) {
            continue;
        }
        if rule.dst_port != 0 && rule.dst_port != dst_port {
            continue;
        }
        LOCK.unlock(saved);
        PASSED.fetch_add(1, Ordering::Relaxed);
        return true;
    }

    LOCK.unlock(saved);
    DROPPED.fetch_add(1, Ordering::Relaxed);
    false
}

pub fn stats() -> (u64, u64) {
    (PASSED.load(Ordering::Relaxed), DROPPED.load(Ordering::Relaxed))
}

pub fn rule_count() -> usize {
    let saved = LOCK.lock();
    let c = unsafe { RULE_COUNT };
    LOCK.unlock(saved);
    c
}

pub fn get_rule(idx: usize) -> Option<FirewallRule> {
    let saved = LOCK.lock();
    let count = unsafe { RULE_COUNT };
    if idx >= count {
        LOCK.unlock(saved);
        return None;
    }
    let rule = unsafe { RULES[idx] };
    LOCK.unlock(saved);
    Some(rule)
}
