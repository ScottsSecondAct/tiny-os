use super::{ethernet, ipv4, Ipv4Addr};
use crate::netbuf;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};

const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_HEADER_LEN: usize = 8;

static PING_SEQ: AtomicU16 = AtomicU16::new(0);
static PING_REPLY_SEQ: AtomicU16 = AtomicU16::new(0xFFFF);
static PING_RTT_TICKS: AtomicU32 = AtomicU32::new(0);
static PING_SEND_TICK: AtomicU32 = AtomicU32::new(0);

pub fn process_rx(buf_idx: u16, ip_hdr: &ipv4::Ipv4Header) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    let icmp_off = ethernet::ETH_HEADER_LEN + ip_hdr.header_len;
    if data.len() < icmp_off + ICMP_HEADER_LEN {
        netbuf::free(buf);
        return;
    }

    let icmp = &data[icmp_off..];
    let icmp_type = icmp[0];
    let _icmp_code = icmp[1];

    match icmp_type {
        ICMP_ECHO_REQUEST => {
            handle_echo_request(buf_idx, ip_hdr, icmp_off);
        }
        ICMP_ECHO_REPLY => {
            let seq = u16::from_be_bytes([icmp[6], icmp[7]]);
            let now = crate::sched::tick_count_32();
            let sent = PING_SEND_TICK.load(Ordering::Relaxed);
            PING_RTT_TICKS.store(now.wrapping_sub(sent), Ordering::Relaxed);
            PING_REPLY_SEQ.store(seq, Ordering::Release);
            netbuf::free(buf);
        }
        _ => {
            netbuf::free(buf);
        }
    }
}

fn handle_echo_request(buf_idx: u16, ip_hdr: &ipv4::Ipv4Header, icmp_off: usize) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    let icmp_len = ip_hdr.total_len as usize - ip_hdr.header_len;
    if data.len() < icmp_off + icmp_len {
        netbuf::free(buf);
        return;
    }

    let reply_buf = match netbuf::alloc() {
        Some(b) => b,
        None => {
            netbuf::free(buf);
            return;
        }
    };

    let icmp_data = &data[icmp_off..icmp_off + icmp_len];
    let mut reply_icmp = [0u8; 128];
    let copy_len = icmp_len.min(128);
    reply_icmp[..copy_len].copy_from_slice(&icmp_data[..copy_len]);
    reply_icmp[0] = ICMP_ECHO_REPLY;
    reply_icmp[1] = 0;
    reply_icmp[2] = 0;
    reply_icmp[3] = 0;
    let csum = ipv4::checksum(&reply_icmp[..copy_len]);
    reply_icmp[2..4].copy_from_slice(&csum.to_be_bytes());

    reply_buf.push_data(&reply_icmp[..copy_len]);
    let reply_idx = netbuf::buf_index(reply_buf);

    let sender = ip_hdr.src;
    netbuf::free(buf);

    ipv4::send(reply_idx, sender, ipv4::PROTO_ICMP, copy_len as u16);
}

pub fn send_ping(dst: Ipv4Addr) -> u16 {
    let seq = PING_SEQ.fetch_add(1, Ordering::Relaxed);

    let buf = match netbuf::alloc() {
        Some(b) => b,
        None => return seq,
    };

    let mut icmp = [0u8; 64];
    icmp[0] = ICMP_ECHO_REQUEST;
    icmp[1] = 0;
    let id: u16 = 0x4F53; // "OS"
    icmp[4..6].copy_from_slice(&id.to_be_bytes());
    icmp[6..8].copy_from_slice(&seq.to_be_bytes());
    for (i, slot) in icmp.iter_mut().enumerate().skip(8) {
        *slot = i as u8;
    }
    icmp[2] = 0;
    icmp[3] = 0;
    let csum = ipv4::checksum(&icmp);
    icmp[2..4].copy_from_slice(&csum.to_be_bytes());

    buf.push_data(&icmp);
    let idx = netbuf::buf_index(buf);

    let now = crate::sched::tick_count_32();
    PING_SEND_TICK.store(now, Ordering::Relaxed);

    ipv4::send(idx, dst, ipv4::PROTO_ICMP, 64);
    seq
}

pub fn ping_result(expected_seq: u16) -> Option<u32> {
    let got = PING_REPLY_SEQ.load(Ordering::Acquire);
    if got == expected_seq {
        Some(PING_RTT_TICKS.load(Ordering::Relaxed))
    } else {
        None
    }
}
