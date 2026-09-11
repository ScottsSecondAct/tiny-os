/// IPv4 internet checksum — one's complement sum with carry folding.
/// Mirrors kernel/src/net/ipv4.rs::checksum().
fn checksum(data: &[u8]) -> u16 {
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Ipv4Addr(pub [u8; 4]);

struct Ipv4Header {
    src: Ipv4Addr,
    dst: Ipv4Addr,
    protocol: u8,
    total_len: u16,
    ttl: u8,
    header_len: usize,
}

fn parse_header(data: &[u8]) -> Option<Ipv4Header> {
    if data.len() < 20 {
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

/// Build a minimal IPv4 header with valid checksum for testing.
fn build_ipv4_header(src: [u8; 4], dst: [u8; 4], proto: u8, payload_len: u16) -> [u8; 20] {
    let total_len = 20u16 + payload_len;
    let mut hdr = [0u8; 20];
    hdr[0] = 0x45; // version=4, IHL=5
    hdr[2..4].copy_from_slice(&total_len.to_be_bytes());
    hdr[4..6].copy_from_slice(&1u16.to_be_bytes()); // ID=1
    hdr[8] = 64; // TTL
    hdr[9] = proto;
    hdr[12..16].copy_from_slice(&src);
    hdr[16..20].copy_from_slice(&dst);
    let csum = checksum(&hdr);
    hdr[10..12].copy_from_slice(&csum.to_be_bytes());
    hdr
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- checksum tests ---

    #[test]
    fn checksum_zeros() {
        let data = [0u8; 20];
        assert_eq!(checksum(&data), 0xFFFF);
    }

    #[test]
    fn checksum_ones() {
        let data = [0xFFu8; 20];
        assert_eq!(checksum(&data), 0x0000);
    }

    #[test]
    fn checksum_rfc1071_example() {
        // RFC 1071 example: 0x0001 + 0x00F2 + ... summed and complemented.
        let data: [u8; 8] = [0x00, 0x01, 0x00, 0xF2, 0x00, 0x03, 0x00, 0x04];
        let csum = checksum(&data);
        // Sum = 0x01 + 0xF2 + 0x03 + 0x04 = 0xFA = 0x00FA, complement = 0xFF05
        assert_eq!(csum, 0xFF05);
    }

    #[test]
    fn checksum_odd_length() {
        // Odd-length input should pad the trailing byte.
        let data = [0x00, 0x01, 0x00, 0xF2, 0xAB];
        let csum_odd = checksum(&data);
        // Same as if padded with 0x00.
        let data_padded = [0x00, 0x01, 0x00, 0xF2, 0xAB, 0x00];
        let csum_even = checksum(&data_padded);
        assert_eq!(csum_odd, csum_even);
    }

    #[test]
    fn checksum_empty() {
        assert_eq!(checksum(&[]), 0xFFFF);
    }

    #[test]
    fn checksum_valid_header_verifies_to_zero() {
        let hdr = build_ipv4_header([10, 0, 0, 1], [10, 0, 0, 2], 6, 100);
        // A valid header's checksum over the full header should be 0.
        assert_eq!(checksum(&hdr), 0);
    }

    #[test]
    fn checksum_detects_corruption() {
        let mut hdr = build_ipv4_header([192, 168, 1, 1], [192, 168, 1, 2], 17, 50);
        assert_eq!(checksum(&hdr), 0);
        // Flip one bit — checksum should no longer verify.
        hdr[15] ^= 0x01;
        assert_ne!(checksum(&hdr), 0);
    }

    // --- parse_header tests ---

    #[test]
    fn parse_valid_header() {
        let hdr = build_ipv4_header([10, 0, 0, 1], [10, 0, 0, 2], 6, 100);
        let parsed = parse_header(&hdr).expect("should parse");
        assert_eq!(parsed.src, Ipv4Addr([10, 0, 0, 1]));
        assert_eq!(parsed.dst, Ipv4Addr([10, 0, 0, 2]));
        assert_eq!(parsed.protocol, 6);
        assert_eq!(parsed.total_len, 120);
        assert_eq!(parsed.ttl, 64);
        assert_eq!(parsed.header_len, 20);
    }

    #[test]
    fn parse_too_short() {
        assert!(parse_header(&[0x45; 19]).is_none());
    }

    #[test]
    fn parse_wrong_version() {
        let mut hdr = build_ipv4_header([1, 2, 3, 4], [5, 6, 7, 8], 1, 0);
        hdr[0] = 0x65; // version 6
        assert!(parse_header(&hdr).is_none());
    }

    #[test]
    fn parse_ihl_too_small() {
        let mut hdr = build_ipv4_header([1, 2, 3, 4], [5, 6, 7, 8], 1, 0);
        hdr[0] = 0x43; // IHL=3 (too small, minimum is 5)
        assert!(parse_header(&hdr).is_none());
    }

    #[test]
    fn parse_corrupted_checksum() {
        let mut hdr = build_ipv4_header([10, 0, 0, 1], [10, 0, 0, 2], 17, 8);
        hdr[10] ^= 0xFF; // corrupt checksum
        assert!(parse_header(&hdr).is_none());
    }

    #[test]
    fn parse_icmp_udp_tcp_protocols() {
        for proto in [1u8, 6, 17] {
            let hdr = build_ipv4_header([127, 0, 0, 1], [127, 0, 0, 1], proto, 0);
            let parsed = parse_header(&hdr).expect("should parse");
            assert_eq!(parsed.protocol, proto);
        }
    }

    #[test]
    fn parse_with_trailing_data() {
        let hdr = build_ipv4_header([10, 0, 0, 1], [10, 0, 0, 2], 6, 100);
        let mut packet = [0u8; 120];
        packet[..20].copy_from_slice(&hdr);
        let parsed = parse_header(&packet).expect("should parse with payload");
        assert_eq!(parsed.header_len, 20);
        assert_eq!(parsed.total_len, 120);
    }
}
