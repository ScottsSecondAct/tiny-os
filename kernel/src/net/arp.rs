use crate::netbuf;
use super::{ethernet, Ipv4Addr};
use core::cell::UnsafeCell;

const ARP_HTYPE_ETH: u16 = 1;
const ARP_PTYPE_IPV4: u16 = 0x0800;
const ARP_OP_REQUEST: u16 = 1;
const ARP_OP_REPLY: u16 = 2;
const ARP_PACKET_LEN: usize = 28;

const MAX_ARP_ENTRIES: usize = 16;

#[derive(Clone, Copy)]
pub struct ArpEntry {
    pub ip: Ipv4Addr,
    pub mac: [u8; 6],
    pub valid: bool,
}

struct ArpCache {
    entries: [ArpEntry; MAX_ARP_ENTRIES],
    count: usize,
}

struct ArpCell(UnsafeCell<ArpCache>);
unsafe impl Sync for ArpCell {}

static ARP_CACHE: ArpCell = ArpCell(UnsafeCell::new(ArpCache {
    entries: [ArpEntry {
        ip: Ipv4Addr::ZERO,
        mac: [0; 6],
        valid: false,
    }; MAX_ARP_ENTRIES],
    count: 0,
}));

fn cache() -> &'static mut ArpCache {
    unsafe { &mut *ARP_CACHE.0.get() }
}

pub fn init() {
    let c = cache();
    c.count = 0;
    for e in c.entries.iter_mut() {
        e.valid = false;
    }
}

pub fn insert(ip: Ipv4Addr, mac: [u8; 6]) {
    let c = cache();
    for e in c.entries.iter_mut() {
        if e.valid && e.ip == ip {
            e.mac = mac;
            return;
        }
    }
    if c.count < MAX_ARP_ENTRIES {
        c.entries[c.count] = ArpEntry { ip, mac, valid: true };
        c.count += 1;
    } else {
        c.entries[0] = ArpEntry { ip, mac, valid: true };
    }
}

pub fn resolve(ip: Ipv4Addr) -> Option<[u8; 6]> {
    let c = cache();
    for e in c.entries.iter() {
        if e.valid && e.ip == ip {
            return Some(e.mac);
        }
    }
    None
}

pub fn cache_entries() -> &'static [ArpEntry] {
    let c = cache();
    &c.entries[..c.count]
}

pub fn process_rx(buf_idx: u16) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    if data.len() < ethernet::ETH_HEADER_LEN + ARP_PACKET_LEN {
        netbuf::free(buf);
        return;
    }

    let arp = &data[ethernet::ETH_HEADER_LEN..];
    let htype = u16::from_be_bytes([arp[0], arp[1]]);
    let ptype = u16::from_be_bytes([arp[2], arp[3]]);
    let hlen = arp[4];
    let plen = arp[5];
    let op = u16::from_be_bytes([arp[6], arp[7]]);

    if htype != ARP_HTYPE_ETH || ptype != ARP_PTYPE_IPV4 || hlen != 6 || plen != 4 {
        netbuf::free(buf);
        return;
    }

    let mut sender_mac = [0u8; 6];
    sender_mac.copy_from_slice(&arp[8..14]);
    let sender_ip = Ipv4Addr([arp[14], arp[15], arp[16], arp[17]]);
    let target_ip = Ipv4Addr([arp[24], arp[25], arp[26], arp[27]]);

    insert(sender_ip, sender_mac);

    if op == ARP_OP_REQUEST && target_ip == super::our_ip() {
        netbuf::free(buf);
        send_reply(sender_ip, &sender_mac);
    } else {
        netbuf::free(buf);
    }
}

fn send_reply(target_ip: Ipv4Addr, target_mac: &[u8; 6]) {
    let buf = match netbuf::alloc() {
        Some(b) => b,
        None => return,
    };

    let our_mac = super::our_mac();
    let our_ip = super::our_ip();

    let mut arp = [0u8; ARP_PACKET_LEN];
    arp[0..2].copy_from_slice(&ARP_HTYPE_ETH.to_be_bytes());
    arp[2..4].copy_from_slice(&ARP_PTYPE_IPV4.to_be_bytes());
    arp[4] = 6;
    arp[5] = 4;
    arp[6..8].copy_from_slice(&ARP_OP_REPLY.to_be_bytes());
    arp[8..14].copy_from_slice(&our_mac);
    arp[14..18].copy_from_slice(&our_ip.0);
    arp[18..24].copy_from_slice(target_mac);
    arp[24..28].copy_from_slice(&target_ip.0);

    buf.push_data(&arp);
    let idx = netbuf::buf_index(buf);
    ethernet::send_frame(idx, target_mac, ethernet::ETHERTYPE_ARP);
}

pub fn send_request(target_ip: Ipv4Addr) {
    let buf = match netbuf::alloc() {
        Some(b) => b,
        None => return,
    };

    let our_mac = super::our_mac();
    let our_ip = super::our_ip();
    let broadcast = [0xFFu8; 6];

    let mut arp = [0u8; ARP_PACKET_LEN];
    arp[0..2].copy_from_slice(&ARP_HTYPE_ETH.to_be_bytes());
    arp[2..4].copy_from_slice(&ARP_PTYPE_IPV4.to_be_bytes());
    arp[4] = 6;
    arp[5] = 4;
    arp[6..8].copy_from_slice(&ARP_OP_REQUEST.to_be_bytes());
    arp[8..14].copy_from_slice(&our_mac);
    arp[14..18].copy_from_slice(&our_ip.0);
    arp[18..24].copy_from_slice(&[0u8; 6]);
    arp[24..28].copy_from_slice(&target_ip.0);

    buf.push_data(&arp);
    let idx = netbuf::buf_index(buf);
    ethernet::send_frame(idx, &broadcast, ethernet::ETHERTYPE_ARP);
}
