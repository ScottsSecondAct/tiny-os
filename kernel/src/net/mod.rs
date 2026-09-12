pub mod arp;
pub mod ethernet;
pub mod firewall;
pub mod icmp;
pub mod ipv4;
pub mod loopback;
pub mod socket;
pub mod tcp;
pub mod udp;

use crate::{kprintln, netbuf};
use core::cell::UnsafeCell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    pub const ZERO: Self = Self([0; 4]);
    pub const LOOPBACK: Self = Self([127, 0, 0, 1]);
    pub const BROADCAST: Self = Self([255, 255, 255, 255]);

    pub fn as_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn from_u32(v: u32) -> Self {
        Self(v.to_be_bytes())
    }
}

impl core::fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

impl core::fmt::Debug for Ipv4Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

struct NetState {
    our_ip: Ipv4Addr,
    our_mac: [u8; 6],
    initialized: bool,
}

struct NetCell(UnsafeCell<NetState>);
unsafe impl Sync for NetCell {}

static NET: NetCell = NetCell(UnsafeCell::new(NetState {
    our_ip: Ipv4Addr::ZERO,
    our_mac: [0; 6],
    initialized: false,
}));

fn state() -> &'static mut NetState {
    unsafe { &mut *NET.0.get() }
}

pub fn init(ip: Ipv4Addr, mac: [u8; 6]) {
    let s = state();
    s.our_ip = ip;
    s.our_mac = mac;
    s.initialized = true;
    arp::init();
    socket::init();
    firewall::init();
    loopback::init(mac);
}

pub fn our_ip() -> Ipv4Addr {
    state().our_ip
}

pub fn our_mac() -> [u8; 6] {
    state().our_mac
}

pub fn process_rx(buf_idx: u16) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };

    if buf.len() < ethernet::ETH_HEADER_LEN {
        netbuf::free(buf);
        return;
    }

    let ethertype = ethernet::parse_ethertype(buf.as_slice());
    match ethertype {
        ethernet::ETHERTYPE_ARP => {
            arp::process_rx(buf_idx);
        }
        ethernet::ETHERTYPE_IPV4 => {
            ipv4::process_rx(buf_idx);
        }
        _ => {
            netbuf::free(buf);
        }
    }
}

pub fn net_task(_arg: usize) -> ! {
    kprintln!("[net] task started");
    loop {
        let dev = loopback::device();
        while let Some(buf_idx) = dev.recv() {
            process_rx(buf_idx);
        }
        crate::sched::delay(1);
    }
}
