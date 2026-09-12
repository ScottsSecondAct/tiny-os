const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        let idx = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = (crc >> 8) ^ CRC32_TABLE[idx];
    }
    crc ^ 0xFFFFFFFF
}

fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut c = crc ^ 0xFFFFFFFF;
    for &byte in data {
        let idx = ((c ^ byte as u32) & 0xFF) as usize;
        c = (c >> 8) ^ CRC32_TABLE[idx];
    }
    c ^ 0xFFFFFFFF
}

#[test]
fn crc32_empty() {
    assert_eq!(crc32(b""), 0x00000000);
}

#[test]
fn crc32_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
}

#[test]
fn crc32_hello() {
    let c = crc32(b"hello");
    assert_ne!(c, 0);
    assert_eq!(c, crc32(b"hello"));
}

#[test]
fn crc32_incremental() {
    let full = crc32(b"helloworld");
    let partial = crc32(b"hello");
    let combined = crc32_update(partial, b"world");
    assert_eq!(full, combined);
}

#[test]
fn crc32_different_inputs() {
    assert_ne!(crc32(b"abc"), crc32(b"abd"));
}

#[test]
fn crc32_single_byte() {
    let c = crc32(&[0x00]);
    assert_eq!(c, 0xD202EF8D);
}
