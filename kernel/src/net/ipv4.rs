use super::{arp, ethernet, Ipv4Addr};
use crate::netbuf;

pub const IPV4_HEADER_LEN: usize = 20;
pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

static mut IP_ID_COUNTER: u16 = 0;

pub struct Ipv4Header {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub protocol: u8,
    pub total_len: u16,
    pub ttl: u8,
    pub header_len: usize,
}

pub fn parse_header(data: &[u8]) -> Option<Ipv4Header> {
    if data.len() < IPV4_HEADER_LEN {
        return None;
    }
    let version = data[0] >> 4;
    let ihl = (data[0] & 0x0F) as usize;
    if version != 4 || ihl < 5 {
        return None;
    }
    let header_len = ihl * 4;
    if data.len() < header_len {
        return None;
    }
    let total_len = u16::from_be_bytes([data[2], data[3]]);
    let ttl = data[8];
    let protocol = data[9];
    let src = Ipv4Addr([data[12], data[13], data[14], data[15]]);
    let dst = Ipv4Addr([data[16], data[17], data[18], data[19]]);

    if checksum(&data[..header_len]) != 0 {
        return None;
    }

    Some(Ipv4Header {
        src,
        dst,
        protocol,
        total_len,
        ttl,
        header_len,
    })
}

pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn process_rx(buf_idx: u16) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    if data.len() < ethernet::ETH_HEADER_LEN + IPV4_HEADER_LEN {
        netbuf::free(buf);
        return;
    }

    let ip_data = &data[ethernet::ETH_HEADER_LEN..];
    let hdr = match parse_header(ip_data) {
        Some(h) => h,
        None => {
            netbuf::free(buf);
            return;
        }
    };

    let our_ip = super::our_ip();
    if hdr.dst != our_ip && hdr.dst != Ipv4Addr::BROADCAST {
        netbuf::free(buf);
        return;
    }

    if !super::firewall::check_packet(ip_data, &hdr) {
        netbuf::free(buf);
        return;
    }

    match hdr.protocol {
        PROTO_ICMP => super::icmp::process_rx(buf_idx, &hdr),
        PROTO_UDP => super::udp::process_rx(buf_idx, &hdr),
        PROTO_TCP => super::tcp::process_rx(buf_idx, &hdr),
        _ => {
            netbuf::free(buf);
        }
    }
}

pub fn send(buf_idx: u16, dst: Ipv4Addr, protocol: u8, payload_len: u16) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let src = super::our_ip();
    let total_len = IPV4_HEADER_LEN as u16 + payload_len;
    let id = unsafe {
        IP_ID_COUNTER += 1;
        IP_ID_COUNTER
    };

    let mut hdr = [0u8; IPV4_HEADER_LEN];
    hdr[0] = 0x45;
    hdr[2..4].copy_from_slice(&total_len.to_be_bytes());
    hdr[4..6].copy_from_slice(&id.to_be_bytes());
    hdr[8] = 64; // TTL
    hdr[9] = protocol;
    hdr[12..16].copy_from_slice(&src.0);
    hdr[16..20].copy_from_slice(&dst.0);
    let csum = checksum(&hdr);
    hdr[10..12].copy_from_slice(&csum.to_be_bytes());

    if !buf.prepend_header(&hdr) {
        netbuf::free(buf);
        return;
    }

    let dst_mac = if dst == Ipv4Addr::BROADCAST || dst == Ipv4Addr::LOOPBACK {
        [0xFF; 6]
    } else {
        match arp::resolve(dst) {
            Some(mac) => mac,
            None => {
                arp::send_request(dst);
                netbuf::free(buf);
                return;
            }
        }
    };

    ethernet::send_frame(buf_idx, &dst_mac, ethernet::ETHERTYPE_IPV4);
}
