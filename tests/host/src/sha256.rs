const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H_INIT: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self { state: H_INIT, buf: [0u8; 64], buf_len: 0, total_len: 0 }
    }

    fn update(&mut self, data: &[u8]) {
        let mut offset = 0;
        self.total_len += data.len() as u64;
        if self.buf_len > 0 {
            let space = 64 - self.buf_len;
            let copy = data.len().min(space);
            self.buf[self.buf_len..self.buf_len + copy].copy_from_slice(&data[..copy]);
            self.buf_len += copy;
            offset = copy;
            if self.buf_len == 64 {
                let block = self.buf;
                compress(&mut self.state, &block);
                self.buf_len = 0;
            }
        }
        while offset + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[offset..offset + 64]);
            compress(&mut self.state, &block);
            offset += 64;
        }
        if offset < data.len() {
            let remaining = data.len() - offset;
            self.buf[..remaining].copy_from_slice(&data[offset..]);
            self.buf_len = remaining;
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            while self.buf_len < 64 { self.buf[self.buf_len] = 0; self.buf_len += 1; }
            let block = self.buf;
            compress(&mut self.state, &block);
            self.buf_len = 0;
            self.buf = [0u8; 64];
        }
        while self.buf_len < 56 { self.buf[self.buf_len] = 0; self.buf_len += 1; }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buf;
        compress(&mut self.state, &block);
        let mut result = [0u8; 32];
        for i in 0..8 {
            result[i * 4..(i + 1) * 4].copy_from_slice(&self.state[i].to_be_bytes());
        }
        result
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update(data);
    ctx.finalize()
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([block[i*4], block[i*4+1], block[i*4+2], block[i*4+3]]);
    }
    for i in 16..64 {
        let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
        let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
        w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
    }
    let (mut a, mut b, mut c, mut d) = (state[0], state[1], state[2], state[3]);
    let (mut e, mut f, mut g, mut h) = (state[4], state[5], state[6], state[7]);
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g; g = f; f = e; e = d.wrapping_add(t1);
        d = c; c = b; b = a; a = t1.wrapping_add(t2);
    }
    for (i, v) in [a, b, c, d, e, f, g, h].iter().enumerate() {
        state[i] = state[i].wrapping_add(*v);
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let h = sha256(key);
        key_block[..32].copy_from_slice(&h);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut i_pad = [0x36u8; 64];
    let mut o_pad = [0x5cu8; 64];
    for i in 0..64 { i_pad[i] ^= key_block[i]; o_pad[i] ^= key_block[i]; }
    let mut inner = Sha256::new();
    inner.update(&i_pad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&o_pad);
    outer.update(&inner_hash);
    outer.finalize()
}

fn to_hex(hash: &[u8; 32]) -> String {
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn sha256_empty() {
    let h = sha256(b"");
    assert_eq!(to_hex(&h), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn sha256_abc() {
    let h = sha256(b"abc");
    assert_eq!(to_hex(&h), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn sha256_448_bits() {
    let h = sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
    assert_eq!(to_hex(&h), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
}

#[test]
fn sha256_long() {
    let h = sha256(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu");
    assert_eq!(to_hex(&h), "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1");
}

#[test]
fn sha256_multi_update() {
    let mut ctx = Sha256::new();
    ctx.update(b"abc");
    ctx.update(b"def");
    let h = ctx.finalize();
    assert_eq!(h, sha256(b"abcdef"));
}

#[test]
fn sha256_tiny_os_password() {
    let h = sha256(b"tiny_os");
    assert_ne!(to_hex(&h), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(h.len(), 32);
}

#[test]
fn hmac_sha256_rfc4231_test1() {
    let key = [0x0bu8; 20];
    let data = b"Hi There";
    let h = hmac_sha256(&key, data);
    assert_eq!(to_hex(&h), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
}

#[test]
fn hmac_sha256_rfc4231_test2() {
    let key = b"Jefe";
    let data = b"what do ya want for nothing?";
    let h = hmac_sha256(key, data);
    assert_eq!(to_hex(&h), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
}

#[test]
fn hmac_verify_correct() {
    let key = b"secret";
    let msg = b"message";
    let tag = hmac_sha256(key, msg);
    let mut diff: u8 = 0;
    for i in 0..32 { diff |= tag[i] ^ tag[i]; }
    assert_eq!(diff, 0);
}

#[test]
fn hmac_verify_wrong() {
    let key = b"secret";
    let tag = hmac_sha256(key, b"message");
    let bad = hmac_sha256(key, b"tampered");
    let mut diff: u8 = 0;
    for i in 0..32 { diff |= tag[i] ^ bad[i]; }
    assert_ne!(diff, 0);
}
