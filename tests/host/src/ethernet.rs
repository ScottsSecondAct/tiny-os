/// Ethernet header parsing — mirrors kernel/src/net/ethernet.rs.

const ETH_HEADER_LEN: usize = 14;
const ETHERTYPE_IPV4: u16 = 0x0800;
const ETHERTYPE_ARP: u16 = 0x0806;

struct EthHeader {
    dst_mac: [u8; 6],
    src_mac: [u8; 6],
    ethertype: u16,
}

fn parse_ethertype(data: &[u8]) -> u16 {
    if data.len() < ETH_HEADER_LEN {
        return 0;
    }
    u16::from_be_bytes([data[12], data[13]])
}

fn parse_header(data: &[u8]) -> Option<EthHeader> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn build_eth_frame(dst: [u8; 6], src: [u8; 6], ethertype: u16, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.extend_from_slice(&dst);
        frame.extend_from_slice(&src);
        frame.extend_from_slice(&ethertype.to_be_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    #[test]
    fn parse_ethertype_ipv4() {
        let frame = build_eth_frame([0xFF; 6], [0x02; 6], ETHERTYPE_IPV4, &[0; 46]);
        assert_eq!(parse_ethertype(&frame), ETHERTYPE_IPV4);
    }

    #[test]
    fn parse_ethertype_arp() {
        let frame = build_eth_frame([0xFF; 6], [0x02; 6], ETHERTYPE_ARP, &[0; 28]);
        assert_eq!(parse_ethertype(&frame), ETHERTYPE_ARP);
    }

    #[test]
    fn parse_ethertype_too_short() {
        assert_eq!(parse_ethertype(&[0u8; 13]), 0);
        assert_eq!(parse_ethertype(&[]), 0);
    }

    #[test]
    fn parse_header_valid() {
        let dst = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let src = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let frame = build_eth_frame(dst, src, ETHERTYPE_IPV4, &[0; 46]);
        let hdr = parse_header(&frame).expect("should parse");
        assert_eq!(hdr.dst_mac, dst);
        assert_eq!(hdr.src_mac, src);
        assert_eq!(hdr.ethertype, ETHERTYPE_IPV4);
    }

    #[test]
    fn parse_header_too_short() {
        assert!(parse_header(&[0u8; 13]).is_none());
        assert!(parse_header(&[]).is_none());
    }

    #[test]
    fn parse_header_exact_minimum() {
        let frame = build_eth_frame([0; 6], [0; 6], 0x8100, &[]);
        assert_eq!(frame.len(), ETH_HEADER_LEN);
        let hdr = parse_header(&frame).expect("14 bytes should be enough");
        assert_eq!(hdr.ethertype, 0x8100);
    }

    #[test]
    fn parse_header_broadcast() {
        let broadcast = [0xFF; 6];
        let src = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        let frame = build_eth_frame(broadcast, src, ETHERTYPE_ARP, &[0; 28]);
        let hdr = parse_header(&frame).unwrap();
        assert_eq!(hdr.dst_mac, broadcast);
        assert_eq!(hdr.src_mac, src);
    }
}
