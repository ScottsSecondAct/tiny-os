use crate::netbuf;
use super::{ethernet, ipv4, Ipv4Addr};
use arch::net::NetError;
use core::cell::UnsafeCell;

const TCP_HEADER_LEN: usize = 20;
const MAX_TCP_CONNS: usize = 4;
const MAX_RX_QUEUE: usize = 4;

const TCP_FIN: u8 = 0x01;
const TCP_SYN: u8 = 0x02;
const TCP_RST: u8 = 0x04;
const TCP_PSH: u8 = 0x08;
const TCP_ACK: u8 = 0x10;

#[derive(Clone, Copy, PartialEq)]
enum TcpState {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    LastAck,
    TimeWait,
}

struct TcpConn {
    state: TcpState,
    local_port: u16,
    remote_port: u16,
    remote_ip: Ipv4Addr,
    send_next: u32,
    send_unack: u32,
    recv_next: u32,
    rx_queue: [u16; MAX_RX_QUEUE],
    rx_head: usize,
    rx_tail: usize,
    rx_count: usize,
}

struct TcpCell(UnsafeCell<TcpInner>);
unsafe impl Sync for TcpCell {}

struct TcpInner {
    conns: [TcpConn; MAX_TCP_CONNS],
    next_port: u16,
}

const UNINIT_CONN: TcpConn = TcpConn {
    state: TcpState::Closed,
    local_port: 0,
    remote_port: 0,
    remote_ip: Ipv4Addr::ZERO,
    send_next: 0,
    send_unack: 0,
    recv_next: 0,
    rx_queue: [0xFFFF; MAX_RX_QUEUE],
    rx_head: 0,
    rx_tail: 0,
    rx_count: 0,
};

static TCP: TcpCell = TcpCell(UnsafeCell::new(TcpInner {
    conns: [UNINIT_CONN; MAX_TCP_CONNS],
    next_port: 49152,
}));

fn inner() -> &'static mut TcpInner {
    unsafe { &mut *TCP.0.get() }
}

pub fn connect(dst: Ipv4Addr, dst_port: u16) -> Result<u8, NetError> {
    let t = inner();
    let slot = t.conns.iter().position(|c| c.state == TcpState::Closed)
        .ok_or(NetError::QueueFull)?;

    let local_port = t.next_port;
    t.next_port = t.next_port.wrapping_add(1).max(49152);

    let isn = initial_seq();
    let conn = &mut t.conns[slot];
    conn.state = TcpState::SynSent;
    conn.local_port = local_port;
    conn.remote_port = dst_port;
    conn.remote_ip = dst;
    conn.send_next = isn.wrapping_add(1);
    conn.send_unack = isn;
    conn.recv_next = 0;
    conn.rx_head = 0;
    conn.rx_tail = 0;
    conn.rx_count = 0;

    send_segment(slot, TCP_SYN, isn, 0, &[]);
    Ok(slot as u8)
}

pub fn send_data(handle: u8, data: &[u8]) -> Result<usize, NetError> {
    let t = inner();
    let conn = &mut t.conns[handle as usize];
    if conn.state != TcpState::Established {
        return Err(NetError::NotConnected);
    }

    let seq = conn.send_next;
    let ack = conn.recv_next;
    let len = data.len().min(1400);
    send_segment(handle as usize, TCP_ACK | TCP_PSH, seq, ack, &data[..len]);
    conn.send_next = seq.wrapping_add(len as u32);
    Ok(len)
}

pub fn recv_data(handle: u8, buf: &mut [u8]) -> Result<usize, NetError> {
    let t = inner();
    let conn = &mut t.conns[handle as usize];
    if conn.state == TcpState::Closed {
        return Err(NetError::NotConnected);
    }
    if conn.rx_count == 0 {
        return Ok(0);
    }

    let idx = conn.rx_queue[conn.rx_head];
    conn.rx_head = (conn.rx_head + 1) % MAX_RX_QUEUE;
    conn.rx_count -= 1;

    let nbuf = match netbuf::get(idx) {
        Some(b) => b,
        None => return Ok(0),
    };
    let data = nbuf.as_slice();
    let copy_len = data.len().min(buf.len());
    buf[..copy_len].copy_from_slice(&data[..copy_len]);
    netbuf::free(nbuf);
    Ok(copy_len)
}

pub fn close(handle: u8) {
    let t = inner();
    let conn = &mut t.conns[handle as usize];
    match conn.state {
        TcpState::Established => {
            let seq = conn.send_next;
            let ack = conn.recv_next;
            send_segment(handle as usize, TCP_FIN | TCP_ACK, seq, ack, &[]);
            conn.send_next = seq.wrapping_add(1);
            conn.state = TcpState::FinWait1;
        }
        TcpState::CloseWait => {
            let seq = conn.send_next;
            let ack = conn.recv_next;
            send_segment(handle as usize, TCP_FIN | TCP_ACK, seq, ack, &[]);
            conn.send_next = seq.wrapping_add(1);
            conn.state = TcpState::LastAck;
        }
        _ => {
            conn.state = TcpState::Closed;
        }
    }
}

pub fn conn_state(handle: u8) -> bool {
    let t = inner();
    t.conns[handle as usize].state == TcpState::Established
}

pub fn process_rx(buf_idx: u16, ip_hdr: &ipv4::Ipv4Header) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    let data = buf.as_slice();
    let tcp_off = ethernet::ETH_HEADER_LEN + ip_hdr.header_len;
    if data.len() < tcp_off + TCP_HEADER_LEN {
        netbuf::free(buf);
        return;
    }

    let tcp = &data[tcp_off..];
    let src_port = u16::from_be_bytes([tcp[0], tcp[1]]);
    let dst_port = u16::from_be_bytes([tcp[2], tcp[3]]);
    let seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
    let ack = u32::from_be_bytes([tcp[8], tcp[9], tcp[10], tcp[11]]);
    let data_offset = ((tcp[12] >> 4) as usize) * 4;
    let flags = tcp[13];

    let t = inner();
    let slot = t.conns.iter().position(|c| {
        c.state != TcpState::Closed
            && c.local_port == dst_port
            && c.remote_port == src_port
            && c.remote_ip == ip_hdr.src
    });

    let slot = match slot {
        Some(s) => s,
        None => {
            netbuf::free(buf);
            return;
        }
    };

    let conn = &mut t.conns[slot];
    match conn.state {
        TcpState::SynSent => {
            if flags & TCP_SYN != 0 && flags & TCP_ACK != 0 {
                conn.recv_next = seq.wrapping_add(1);
                conn.send_unack = ack;
                conn.state = TcpState::Established;
                send_segment(slot, TCP_ACK, conn.send_next, conn.recv_next, &[]);
            }
            netbuf::free(buf);
        }
        TcpState::Established => {
            let payload_off = tcp_off + data_offset;
            let payload_len = (ip_hdr.total_len as usize)
                .saturating_sub(ip_hdr.header_len + data_offset);

            if flags & TCP_FIN != 0 {
                conn.recv_next = seq.wrapping_add(payload_len as u32).wrapping_add(1);
                conn.state = TcpState::CloseWait;
                send_segment(slot, TCP_ACK, conn.send_next, conn.recv_next, &[]);
                netbuf::free(buf);
            } else if payload_len > 0 {
                conn.recv_next = seq.wrapping_add(payload_len as u32);
                send_segment(slot, TCP_ACK, conn.send_next, conn.recv_next, &[]);

                let nbuf = buf;
                nbuf.head = payload_off as u16;
                nbuf.tail = (payload_off + payload_len) as u16;
                if conn.rx_count < MAX_RX_QUEUE {
                    conn.rx_queue[conn.rx_tail] = buf_idx;
                    conn.rx_tail = (conn.rx_tail + 1) % MAX_RX_QUEUE;
                    conn.rx_count += 1;
                } else {
                    netbuf::free(nbuf);
                }
            } else {
                if flags & TCP_ACK != 0 {
                    conn.send_unack = ack;
                }
                netbuf::free(buf);
            }
        }
        TcpState::FinWait1 => {
            if flags & TCP_ACK != 0 {
                conn.state = TcpState::FinWait2;
            }
            if flags & TCP_FIN != 0 {
                conn.recv_next = seq.wrapping_add(1);
                send_segment(slot, TCP_ACK, conn.send_next, conn.recv_next, &[]);
                conn.state = TcpState::Closed;
            }
            netbuf::free(buf);
        }
        TcpState::FinWait2 => {
            if flags & TCP_FIN != 0 {
                conn.recv_next = seq.wrapping_add(1);
                send_segment(slot, TCP_ACK, conn.send_next, conn.recv_next, &[]);
                conn.state = TcpState::Closed;
            }
            netbuf::free(buf);
        }
        TcpState::LastAck => {
            if flags & TCP_ACK != 0 {
                conn.state = TcpState::Closed;
            }
            netbuf::free(buf);
        }
        _ => {
            netbuf::free(buf);
        }
    }
}

fn send_segment(slot: usize, flags: u8, seq: u32, ack: u32, payload: &[u8]) {
    let t = inner();
    let conn = &t.conns[slot];

    let buf = match netbuf::alloc() {
        Some(b) => b,
        None => return,
    };

    if !payload.is_empty() {
        buf.push_data(payload);
    }

    let data_offset: u8 = 5;
    let mut hdr = [0u8; TCP_HEADER_LEN];
    hdr[0..2].copy_from_slice(&conn.local_port.to_be_bytes());
    hdr[2..4].copy_from_slice(&conn.remote_port.to_be_bytes());
    hdr[4..8].copy_from_slice(&seq.to_be_bytes());
    hdr[8..12].copy_from_slice(&ack.to_be_bytes());
    hdr[12] = data_offset << 4;
    hdr[13] = flags;
    hdr[14..16].copy_from_slice(&8192u16.to_be_bytes()); // window

    if !buf.prepend_header(&hdr) {
        netbuf::free(buf);
        return;
    }

    let total_len = TCP_HEADER_LEN + payload.len();
    let idx = netbuf::buf_index(buf);
    ipv4::send(idx, conn.remote_ip, ipv4::PROTO_TCP, total_len as u16);
}

fn initial_seq() -> u32 {
    let ticks = crate::sched::tick_count_32();
    ticks.wrapping_mul(64000)
}
