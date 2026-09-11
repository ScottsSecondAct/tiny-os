use super::sha256::{self, Sha256};

const BLOCK_SIZE: usize = 64;
const HASH_SIZE: usize = 32;

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; HASH_SIZE] {
    let mut key_block = [0u8; BLOCK_SIZE];

    if key.len() > BLOCK_SIZE {
        let hashed = sha256::hash(key);
        key_block[..HASH_SIZE].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut i_pad = [0x36u8; BLOCK_SIZE];
    let mut o_pad = [0x5cu8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        i_pad[i] ^= key_block[i];
        o_pad[i] ^= key_block[i];
    }

    let mut inner = Sha256::new();
    inner.update(&i_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&o_pad);
    outer.update(&inner_hash);
    outer.finalize()
}

pub fn verify(key: &[u8], message: &[u8], expected_tag: &[u8; HASH_SIZE]) -> bool {
    let computed = hmac_sha256(key, message);
    constant_time_eq(&computed, expected_tag)
}

fn constant_time_eq(a: &[u8; HASH_SIZE], b: &[u8; HASH_SIZE]) -> bool {
    let mut diff: u8 = 0;
    for i in 0..HASH_SIZE {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

pub fn constant_time_eq_pub(a: &[u8; HASH_SIZE], b: &[u8; HASH_SIZE]) -> bool {
    constant_time_eq(a, b)
}
