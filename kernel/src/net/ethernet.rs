use crate::netbuf::{self, NetBuf};

pub const ETH_HEADER_LEN: usize = 14;
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;

pub struct EthHeader {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

pub fn parse_ethertype(data: &[u8]) -> u16 {
    if data.len() < ETH_HEADER_LEN {
        return 0;
    }
    u16::from_be_bytes([data[12], data[13]])
}

pub fn parse_header(data: &[u8]) -> Option<EthHeader> {
    if data.len() < ETH_HEADER_LEN {
        return None;
    }
    let mut dst_mac = [0u8; 6];
    let mut src_mac = [0u8; 6];
    dst_mac.copy_from_slice(&data[0..6]);
    src_mac.copy_from_slice(&data[6..12]);
    Some(EthHeader {
        dst_mac,
        src_mac,
        ethertype: u16::from_be_bytes([data[12], data[13]]),
    })
}

pub fn strip_header(buf: &mut NetBuf) {
    buf.head += ETH_HEADER_LEN as u16;
}

pub fn prepend_header(
    buf: &mut NetBuf,
    dst_mac: &[u8; 6],
    src_mac: &[u8; 6],
    ethertype: u16,
) -> bool {
    let mut hdr = [0u8; ETH_HEADER_LEN];
    hdr[0..6].copy_from_slice(dst_mac);
    hdr[6..12].copy_from_slice(src_mac);
    hdr[12..14].copy_from_slice(&ethertype.to_be_bytes());
    buf.prepend_header(&hdr)
}

pub fn send_frame(buf_idx: u16, dst_mac: &[u8; 6], ethertype: u16) {
    let buf = match netbuf::get(buf_idx) {
        Some(b) => b,
        None => return,
    };
    let src_mac = super::our_mac();
    if !prepend_header(buf, dst_mac, &src_mac, ethertype) {
        netbuf::free(buf);
        return;
    }
    let dev = super::loopback::device();
    if dev.send(buf_idx).is_err() {
        let buf = netbuf::get(buf_idx).unwrap();
        netbuf::free(buf);
    }
}
