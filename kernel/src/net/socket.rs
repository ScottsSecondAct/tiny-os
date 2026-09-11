use super::{tcp, udp, ipv4, Ipv4Addr};
use arch::net::NetError;
use core::cell::UnsafeCell;

const MAX_SOCKETS: usize = 8;

#[derive(Clone, Copy, PartialEq)]
pub enum SockType {
    Udp,
    Tcp,
}

#[derive(Clone, Copy, PartialEq)]
enum SockState {
    Free,
    Open,
    Bound,
    Connected,
}

struct Socket {
    state: SockState,
    sock_type: SockType,
    local_port: u16,
    remote_ip: Ipv4Addr,
    remote_port: u16,
    proto_handle: u8,
}

struct SocketTable {
    sockets: [Socket; MAX_SOCKETS],
}

struct SockCell(UnsafeCell<SocketTable>);
unsafe impl Sync for SockCell {}

const UNINIT_SOCKET: Socket = Socket {
    state: SockState::Free,
    sock_type: SockType::Udp,
    local_port: 0,
    remote_ip: Ipv4Addr::ZERO,
    remote_port: 0,
    proto_handle: 0xFF,
};

static SOCKETS: SockCell = SockCell(UnsafeCell::new(SocketTable {
    sockets: [UNINIT_SOCKET; MAX_SOCKETS],
}));

fn table() -> &'static mut SocketTable {
    unsafe { &mut *SOCKETS.0.get() }
}

pub fn init() {
    let t = table();
    for s in t.sockets.iter_mut() {
        s.state = SockState::Free;
    }
}

pub fn socket(sock_type: SockType) -> Result<u8, NetError> {
    let t = table();
    for (i, s) in t.sockets.iter_mut().enumerate() {
        if s.state == SockState::Free {
            s.state = SockState::Open;
            s.sock_type = sock_type;
            s.local_port = 0;
            s.remote_ip = Ipv4Addr::ZERO;
            s.remote_port = 0;
            s.proto_handle = 0xFF;
            return Ok(i as u8);
        }
    }
    Err(NetError::QueueFull)
}

pub fn bind(fd: u8, port: u16) -> Result<(), NetError> {
    let t = table();
    let s = &mut t.sockets[fd as usize];
    if s.state == SockState::Free {
        return Err(NetError::BadFd);
    }
    match s.sock_type {
        SockType::Udp => {
            let handle = udp::bind(port).ok_or(NetError::AddrInUse)?;
            s.proto_handle = handle;
            s.local_port = port;
            s.state = SockState::Bound;
            Ok(())
        }
        SockType::Tcp => {
            s.local_port = port;
            s.state = SockState::Bound;
            Ok(())
        }
    }
}

pub fn connect(fd: u8, addr: Ipv4Addr, port: u16) -> Result<(), NetError> {
    let t = table();
    let s = &mut t.sockets[fd as usize];
    if s.state == SockState::Free {
        return Err(NetError::BadFd);
    }
    match s.sock_type {
        SockType::Tcp => {
            let handle = tcp::connect(addr, port)?;
            s.proto_handle = handle;
            s.remote_ip = addr;
            s.remote_port = port;
            s.state = SockState::Connected;
            Ok(())
        }
        SockType::Udp => {
            s.remote_ip = addr;
            s.remote_port = port;
            s.state = SockState::Connected;
            Ok(())
        }
    }
}

pub fn sendto(fd: u8, data: &[u8], addr: Ipv4Addr, port: u16) -> Result<usize, NetError> {
    let t = table();
    let s = &t.sockets[fd as usize];
    if s.state == SockState::Free {
        return Err(NetError::BadFd);
    }
    match s.sock_type {
        SockType::Udp => {
            let src_port = s.local_port;
            udp::send(addr, port, src_port, data);
            Ok(data.len())
        }
        SockType::Tcp => {
            tcp::send_data(s.proto_handle, data)
        }
    }
}

pub fn send(fd: u8, data: &[u8]) -> Result<usize, NetError> {
    let t = table();
    let s = &t.sockets[fd as usize];
    if s.state == SockState::Free {
        return Err(NetError::BadFd);
    }
    sendto(fd, data, s.remote_ip, s.remote_port)
}

pub fn recvfrom(fd: u8, buf: &mut [u8]) -> Result<(usize, Ipv4Addr, u16), NetError> {
    let t = table();
    let s = &t.sockets[fd as usize];
    if s.state == SockState::Free {
        return Err(NetError::BadFd);
    }
    match s.sock_type {
        SockType::Udp => {
            match udp::recv(s.proto_handle) {
                Some((buf_idx, src_ip, src_port)) => {
                    let nbuf = netbuf::get(buf_idx).ok_or(NetError::InvalidBuf)?;
                    let data = nbuf.as_slice();
                    let eth_ip_udp_hdr = super::ethernet::ETH_HEADER_LEN
                        + ipv4::IPV4_HEADER_LEN + 8;
                    if data.len() > eth_ip_udp_hdr {
                        let payload = &data[eth_ip_udp_hdr..];
                        let copy_len = payload.len().min(buf.len());
                        buf[..copy_len].copy_from_slice(&payload[..copy_len]);
                        crate::netbuf::free(nbuf);
                        Ok((copy_len, src_ip, src_port))
                    } else {
                        crate::netbuf::free(nbuf);
                        Ok((0, src_ip, src_port))
                    }
                }
                None => Ok((0, Ipv4Addr::ZERO, 0)),
            }
        }
        SockType::Tcp => {
            let n = tcp::recv_data(s.proto_handle, buf)?;
            Ok((n, s.remote_ip, s.remote_port))
        }
    }
}

pub fn close(fd: u8) {
    let t = table();
    let s = &mut t.sockets[fd as usize];
    if s.state == SockState::Free {
        return;
    }
    match s.sock_type {
        SockType::Udp => {
            if s.proto_handle != 0xFF {
                udp::unbind(s.proto_handle);
            }
        }
        SockType::Tcp => {
            if s.proto_handle != 0xFF {
                tcp::close(s.proto_handle);
            }
        }
    }
    s.state = SockState::Free;
}

pub fn socket_count() -> usize {
    let t = table();
    t.sockets.iter().filter(|s| s.state != SockState::Free).count()
}

use crate::netbuf;
