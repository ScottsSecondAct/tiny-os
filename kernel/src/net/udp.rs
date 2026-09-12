use super::{ethernet, ipv4, Ipv4Addr};
use crate::netbuf;
use core::cell::UnsafeCell;

const UDP_HEADER_LEN: usize = 8;
const MAX_UDP_SOCKETS: usize = 8;
const MAX_RX_QUEUE: usize = 8;

struct UdpSocket {
    bound: bool,
    port: u16,
    rx_queue: [u16; MAX_RX_QUEUE],
    rx_head: usize,
    rx_tail: usize,
    rx_count: usize,
    rx_src_ip: [Ipv4Addr; MAX_RX_QUEUE],
    rx_src_port: [u16; MAX_RX_QUEUE],
}

struct UdpState {
    sockets: [UdpSocket; MAX_UDP_SOCKETS],
}

struct UdpCell(UnsafeCell<UdpState>);
unsafe impl Sync for UdpCell {}

const UNINIT_SOCKET: UdpSocket = UdpSocket {
    bound: false,
    port: 0,
    rx_queue: [0xFFFF; MAX_RX_QUEUE],
    rx_head: 0,
    rx_tail: 0,
    rx_count: 0,
    rx_src_ip: [Ipv4Addr::ZERO; MAX_RX_QUEUE],
    rx_src_port: [0; MAX_RX_QUEUE],
};

static UDP: UdpCell = UdpCell(UnsafeCell::new(UdpState {
    sockets: [UNINIT_SOCKET; MAX_UDP_SOCKETS],
}));

fn state() -> &'static mut UdpState {
    unsafe { &mut *UDP.0.get() }
}

pub fn bind(port: u16) -> Option<u8> {
    let s = state();
    for (i, sock) in s.sockets.iter_mut().enumerate() {
        if !sock.bound {
            sock.bound = true;
            sock.port = port;
            sock.rx_head = 0;
            sock.rx_tail = 0;
            sock.rx_count = 0;
            return Some(i as u8);
        }
    }
    None
}

pub fn unbind(handle: u8) {
    let s = state();
    if (handle as usize) < MAX_UDP_SOCKETS {
        let sock = &mut s.sockets[handle as usize];
        while sock.rx_count > 0 {
            let idx = sock.rx_queue[sock.rx_head];
            if let Some(buf) = netbuf::get(idx) {
                netbuf::free(buf);
            }
            sock.rx_head = (sock.rx_head + 1) % MAX_RX_QUEUE;
            sock.rx_count -= 1;
        }
        sock.bound = false;
    }
}

pub fn recv(handle: u8) -> Option<(u16, Ipv4Addr, u16)> {
    let s = state();
    if (handle as usize) >= MAX_UDP_SOCKETS {
        return None;
    }
    let sock = &mut s.sockets[handle as usize];
    if !sock.bound || sock.rx_count == 0 {
        return None;
    }
    let idx = sock.rx_queue[sock.rx_head];
    let src_ip = sock.rx_src_ip[sock.rx_head];
    let src_port = sock.rx_src_port[sock.rx_head];
    sock.rx_head = (sock.rx_head + 1) % MAX_RX_QUEUE;
    sock.rx_count -= 1;
    Some((idx, src_ip, src_port))
}

pub fn process_rx(buf_idx: u16, ip_hdr: &ipv4::Ipv4Header) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    let udp_off = ethernet::ETH_HEADER_LEN + ip_hdr.header_len;
    if data.len() < udp_off + UDP_HEADER_LEN {
        netbuf::free(buf);
        return;
    }

    let udp = &data[udp_off..];
    let _src_port = u16::from_be_bytes([udp[0], udp[1]]);
    let dst_port = u16::from_be_bytes([udp[2], udp[3]]);
    let udp_len = u16::from_be_bytes([udp[4], udp[5]]) as usize;

    if data.len() < udp_off + udp_len {
        netbuf::free(buf);
        return;
    }

    let s = state();
    for sock in s.sockets.iter_mut() {
        if sock.bound && sock.port == dst_port && sock.rx_count < MAX_RX_QUEUE {
            sock.rx_queue[sock.rx_tail] = buf_idx;
            sock.rx_src_ip[sock.rx_tail] = ip_hdr.src;
            sock.rx_src_port[sock.rx_tail] = _src_port;
            sock.rx_tail = (sock.rx_tail + 1) % MAX_RX_QUEUE;
            sock.rx_count += 1;
            return;
        }
    }

    netbuf::free(buf);
}

pub fn send(dst: Ipv4Addr, dst_port: u16, src_port: u16, payload: &[u8]) {
    let buf = match netbuf::alloc() {
        Some(b) => b,
        None => return,
    };

    buf.push_data(payload);
    let udp_len = (UDP_HEADER_LEN + payload.len()) as u16;

    let mut hdr = [0u8; UDP_HEADER_LEN];
    hdr[0..2].copy_from_slice(&src_port.to_be_bytes());
    hdr[2..4].copy_from_slice(&dst_port.to_be_bytes());
    hdr[4..6].copy_from_slice(&udp_len.to_be_bytes());

    if !buf.prepend_header(&hdr) {
        netbuf::free(buf);
        return;
    }

    let idx = netbuf::buf_index(buf);
    ipv4::send(idx, dst, ipv4::PROTO_UDP, udp_len);
}
