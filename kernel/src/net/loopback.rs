use crate::netbuf;
use arch::net::{NetDevice, NetError};
use core::cell::UnsafeCell;

const RX_RING_SIZE: usize = 16;

struct Loopback {
    mac: [u8; 6],
    rx_ring: [u16; RX_RING_SIZE],
    rx_head: usize,
    rx_tail: usize,
    rx_count: usize,
}

struct LoopbackCell(UnsafeCell<Loopback>);
unsafe impl Sync for LoopbackCell {}

static LO: LoopbackCell = LoopbackCell(UnsafeCell::new(Loopback {
    mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
    rx_ring: [0xFFFF; RX_RING_SIZE],
    rx_head: 0,
    rx_tail: 0,
    rx_count: 0,
}));

fn lo() -> &'static mut Loopback {
    unsafe { &mut *LO.0.get() }
}

pub fn init(mac: [u8; 6]) {
    let l = lo();
    l.mac = mac;
    l.rx_head = 0;
    l.rx_tail = 0;
    l.rx_count = 0;
}

pub fn device() -> &'static mut dyn NetDevice {
    lo()
}

impl NetDevice for Loopback {
    fn send(&mut self, buf_idx: u16) -> Result<(), NetError> {
        let buf = match netbuf::get(buf_idx) {
            Some(b) => b,
            None => return Err(NetError::InvalidBuf),
        };

        let data = buf.as_slice();
        if data.len() < 14 {
            netbuf::free(buf);
            return Ok(());
        }

        let reply = match netbuf::alloc() {
            Some(b) => b,
            None => {
                netbuf::free(buf);
                return Err(NetError::QueueFull);
            }
        };

        let pkt = buf.as_slice();
        let mut modified = [0u8; 1536];
        let pkt_len = pkt.len().min(1536);
        modified[..pkt_len].copy_from_slice(&pkt[..pkt_len]);

        modified[0..6].copy_from_slice(&pkt[6..12]);
        modified[6..12].copy_from_slice(&pkt[0..6]);

        let ethertype = u16::from_be_bytes([modified[12], modified[13]]);
        if ethertype == 0x0800 && pkt_len >= 34 {
            let (left, right) = modified.split_at_mut(30);
            let mut tmp = [0u8; 4];
            tmp.copy_from_slice(&left[26..30]);
            left[26..30].copy_from_slice(&right[..4]);
            right[..4].copy_from_slice(&tmp);

            let ihl = (modified[14] & 0x0F) as usize * 4;
            modified[24] = 0;
            modified[24 + 1] = 0;
            let ip_csum = super::ipv4::checksum(&modified[14..14 + ihl]);
            modified[24..26].copy_from_slice(&ip_csum.to_be_bytes());

            let proto = modified[23];
            let ip_payload_off = 14 + ihl;
            if proto == 1 && pkt_len > ip_payload_off && modified[ip_payload_off] == 8 {
                modified[ip_payload_off] = 0;
                let icmp_end = pkt_len;
                modified[ip_payload_off + 2] = 0;
                modified[ip_payload_off + 3] = 0;
                let icmp_csum = super::ipv4::checksum(&modified[ip_payload_off..icmp_end]);
                modified[ip_payload_off + 2..ip_payload_off + 4]
                    .copy_from_slice(&icmp_csum.to_be_bytes());
            }
        }

        reply.push_data(&modified[..pkt_len]);
        let reply_idx = netbuf::buf_index(reply);

        netbuf::free(buf);

        if self.rx_count < RX_RING_SIZE {
            self.rx_ring[self.rx_tail] = reply_idx;
            self.rx_tail = (self.rx_tail + 1) % RX_RING_SIZE;
            self.rx_count += 1;
            Ok(())
        } else {
            let rbuf = netbuf::get(reply_idx).unwrap();
            netbuf::free(rbuf);
            Err(NetError::QueueFull)
        }
    }

    fn recv(&mut self) -> Option<u16> {
        if self.rx_count == 0 {
            return None;
        }
        let idx = self.rx_ring[self.rx_head];
        self.rx_head = (self.rx_head + 1) % RX_RING_SIZE;
        self.rx_count -= 1;
        Some(idx)
    }

    fn mac_addr(&self) -> [u8; 6] {
        self.mac
    }

    fn has_link(&self) -> bool {
        true
    }
}
