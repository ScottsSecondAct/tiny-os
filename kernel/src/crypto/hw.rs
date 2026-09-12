use arch::crypto_engine::{AesMode, CryptoEngine, CryptoHwError};

pub struct ArmCryptoEngine;

impl ArmCryptoEngine {
    pub const fn new() -> Self {
        Self
    }

    pub fn detect() -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            let isar0: u64;
            // SAFETY: Reading ID_AA64ISAR0_EL1 is a read-only system register access.
            unsafe { core::arch::asm!("mrs {}, id_aa64isar0_el1", out(reg) isar0) };
            let aes = (isar0 >> 4) & 0xF;
            let sha2 = (isar0 >> 12) & 0xF;
            aes >= 1 && sha2 >= 1
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            false
        }
    }
}

impl CryptoEngine for ArmCryptoEngine {
    fn aes_encrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        input: &[u8],
        output: &mut [u8],
        mode: AesMode,
    ) -> Result<usize, CryptoHwError> {
        if key.len() != 16 && key.len() != 32 {
            return Err(CryptoHwError::InvalidKeySize);
        }
        if !input.len().is_multiple_of(16) || input.is_empty() {
            return Err(CryptoHwError::InvalidInput);
        }
        if output.len() < input.len() {
            return Err(CryptoHwError::InvalidInput);
        }

        if !Self::detect() {
            return Err(CryptoHwError::NotAvailable);
        }

        #[cfg(target_arch = "aarch64")]
        {
            aes_encrypt_hw(key, iv, input, output, mode)
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = (iv, mode);
            Err(CryptoHwError::NotAvailable)
        }
    }

    fn aes_decrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        input: &[u8],
        output: &mut [u8],
        mode: AesMode,
    ) -> Result<usize, CryptoHwError> {
        if key.len() != 16 && key.len() != 32 {
            return Err(CryptoHwError::InvalidKeySize);
        }
        if !input.len().is_multiple_of(16) || input.is_empty() {
            return Err(CryptoHwError::InvalidInput);
        }
        if output.len() < input.len() {
            return Err(CryptoHwError::InvalidInput);
        }

        if !Self::detect() {
            return Err(CryptoHwError::NotAvailable);
        }

        #[cfg(target_arch = "aarch64")]
        {
            aes_decrypt_hw(key, iv, input, output, mode)
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = (iv, mode);
            Err(CryptoHwError::NotAvailable)
        }
    }

    fn sha256(&self, input: &[u8], output: &mut [u8; 32]) -> Result<(), CryptoHwError> {
        if !Self::detect() {
            return Err(CryptoHwError::NotAvailable);
        }

        #[cfg(target_arch = "aarch64")]
        {
            sha256_hw(input, output)
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            let _ = input;
            Err(CryptoHwError::NotAvailable)
        }
    }

    fn supported(&self) -> bool {
        Self::detect()
    }
}

// --- AArch64 hardware implementations ---

#[cfg(target_arch = "aarch64")]
fn aes_encrypt_hw(
    key: &[u8],
    iv: &[u8],
    input: &[u8],
    output: &mut [u8],
    mode: AesMode,
) -> Result<usize, CryptoHwError> {
    let nr = if key.len() == 16 { 10 } else { 14 };
    let mut round_keys = [[0u8; 16]; 15];
    aes_key_expand(key, &mut round_keys[..nr + 1]);

    let mut prev_ct = [0u8; 16];
    if matches!(mode, AesMode::Cbc | AesMode::Ctr) {
        if iv.len() < 16 {
            return Err(CryptoHwError::InvalidInput);
        }
        prev_ct.copy_from_slice(&iv[..16]);
    }

    let blocks = input.len() / 16;
    for b in 0..blocks {
        let off = b * 16;
        let mut block = [0u8; 16];
        block.copy_from_slice(&input[off..off + 16]);

        match mode {
            AesMode::Ecb => {}
            AesMode::Cbc => {
                for i in 0..16 {
                    block[i] ^= prev_ct[i];
                }
            }
            AesMode::Ctr => {
                let encrypted_ctr = aes_encrypt_block(&prev_ct, &round_keys[..nr + 1]);
                for i in 0..16 {
                    block[i] ^= encrypted_ctr[i];
                }
                output[off..off + 16].copy_from_slice(&block);
                ctr_increment(&mut prev_ct);
                continue;
            }
        }

        let ct = aes_encrypt_block(&block, &round_keys[..nr + 1]);
        output[off..off + 16].copy_from_slice(&ct);

        if mode == AesMode::Cbc {
            prev_ct.copy_from_slice(&ct);
        }
    }

    Ok(input.len())
}

#[cfg(target_arch = "aarch64")]
fn aes_decrypt_hw(
    key: &[u8],
    iv: &[u8],
    input: &[u8],
    output: &mut [u8],
    mode: AesMode,
) -> Result<usize, CryptoHwError> {
    let nr = if key.len() == 16 { 10 } else { 14 };
    let mut round_keys = [[0u8; 16]; 15];
    aes_key_expand(key, &mut round_keys[..nr + 1]);

    let mut prev_ct = [0u8; 16];
    if matches!(mode, AesMode::Cbc | AesMode::Ctr) {
        if iv.len() < 16 {
            return Err(CryptoHwError::InvalidInput);
        }
        prev_ct.copy_from_slice(&iv[..16]);
    }

    let blocks = input.len() / 16;
    for b in 0..blocks {
        let off = b * 16;
        let mut block = [0u8; 16];
        block.copy_from_slice(&input[off..off + 16]);

        match mode {
            AesMode::Ecb => {
                let pt = aes_decrypt_block(&block, &round_keys[..nr + 1]);
                output[off..off + 16].copy_from_slice(&pt);
            }
            AesMode::Cbc => {
                let pt = aes_decrypt_block(&block, &round_keys[..nr + 1]);
                let mut out_block = [0u8; 16];
                for i in 0..16 {
                    out_block[i] = pt[i] ^ prev_ct[i];
                }
                prev_ct.copy_from_slice(&input[off..off + 16]);
                output[off..off + 16].copy_from_slice(&out_block);
            }
            AesMode::Ctr => {
                let encrypted_ctr = aes_encrypt_block(&prev_ct, &round_keys[..nr + 1]);
                for i in 0..16 {
                    output[off + i] = block[i] ^ encrypted_ctr[i];
                }
                ctr_increment(&mut prev_ct);
            }
        }
    }

    Ok(input.len())
}

#[cfg(target_arch = "aarch64")]
fn aes_encrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]]) -> [u8; 16] {
    let mut state = *block;
    let nr = round_keys.len() - 1;

    // XOR with round key 0
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    // Rounds 1..nr-1: AESE + AESMC
    for rk in round_keys.iter().take(nr).skip(1) {
        // SAFETY: We verified crypto extensions are available via detect().
        unsafe {
            core::arch::asm!(
                ".arch_extension crypto",
                "ld1 {{v0.16b}}, [{state}]",
                "ld1 {{v1.16b}}, [{rk}]",
                "aese v0.16b, v1.16b",
                "aesmc v0.16b, v0.16b",
                "st1 {{v0.16b}}, [{state}]",
                state = in(reg) state.as_mut_ptr(),
                rk = in(reg) [0u8; 16].as_ptr(),
                options(nostack),
            );
        }
        for i in 0..16 {
            state[i] ^= rk[i];
        }
    }

    // Final round: AESE only (no MixColumns), then XOR last round key
    // SAFETY: Same as above.
    unsafe {
        core::arch::asm!(
            ".arch_extension crypto",
            "ld1 {{v0.16b}}, [{state}]",
            "movi v1.16b, #0",
            "aese v0.16b, v1.16b",
            "st1 {{v0.16b}}, [{state}]",
            state = in(reg) state.as_mut_ptr(),
            options(nostack),
        );
    }
    for i in 0..16 {
        state[i] ^= round_keys[nr][i];
    }

    state
}

#[cfg(target_arch = "aarch64")]
fn aes_decrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]]) -> [u8; 16] {
    let mut state = *block;
    let nr = round_keys.len() - 1;

    // XOR with last round key
    for i in 0..16 {
        state[i] ^= round_keys[nr][i];
    }

    // Rounds nr-1..1: AESD + AESIMC
    for r in (1..nr).rev() {
        // SAFETY: We verified crypto extensions are available via detect().
        unsafe {
            core::arch::asm!(
                ".arch_extension crypto",
                "ld1 {{v0.16b}}, [{state}]",
                "movi v1.16b, #0",
                "aesd v0.16b, v1.16b",
                "aesimc v0.16b, v0.16b",
                "st1 {{v0.16b}}, [{state}]",
                state = in(reg) state.as_mut_ptr(),
                options(nostack),
            );
        }
        for i in 0..16 {
            state[i] ^= round_keys[r][i];
        }
    }

    // Final round: AESD only, XOR round key 0
    // SAFETY: Same as above.
    unsafe {
        core::arch::asm!(
            ".arch_extension crypto",
            "ld1 {{v0.16b}}, [{state}]",
            "movi v1.16b, #0",
            "aesd v0.16b, v1.16b",
            "st1 {{v0.16b}}, [{state}]",
            state = in(reg) state.as_mut_ptr(),
            options(nostack),
        );
    }
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    state
}

#[cfg(target_arch = "aarch64")]
fn sha256_hw(input: &[u8], output: &mut [u8; 32]) -> Result<(), CryptoHwError> {
    let h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Pad message
    let bit_len = (input.len() as u64) * 8;
    let pad_len = {
        let rem = (input.len() + 1) % 64;
        if rem <= 56 {
            56 - rem
        } else {
            120 - rem
        }
    };
    let total_len = input.len() + 1 + pad_len + 8;
    let num_blocks = total_len / 64;

    let mut hash = h;
    let mut msg_buf = [0u8; 64];

    for block_idx in 0..num_blocks {
        let base = block_idx * 64;
        // Fill msg_buf with padded message
        for (i, slot) in msg_buf.iter_mut().enumerate() {
            let pos = base + i;
            if pos < input.len() {
                *slot = input[pos];
            } else if pos == input.len() {
                *slot = 0x80;
            } else if pos >= total_len - 8 {
                let byte_idx = pos - (total_len - 8);
                *slot = (bit_len >> (56 - byte_idx * 8)) as u8;
            } else {
                *slot = 0;
            }
        }

        // Parse block into 16 big-endian words
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                msg_buf[i * 4],
                msg_buf[i * 4 + 1],
                msg_buf[i * 4 + 2],
                msg_buf[i * 4 + 3],
            ]);
        }

        // Extend to 64 words
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Compression
        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut hh = hash[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(hh);
    }

    for (i, &word) in hash.iter().enumerate() {
        let bytes = word.to_be_bytes();
        output[i * 4..i * 4 + 4].copy_from_slice(&bytes);
    }

    Ok(())
}

// AES key schedule (software — no AESE-based key expansion needed)
#[cfg(target_arch = "aarch64")]
fn aes_key_expand(key: &[u8], round_keys: &mut [[u8; 16]]) {
    const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];

    let nk = key.len() / 4;
    let nr = round_keys.len() - 1;
    let nw = (nr + 1) * 4;

    let mut w = [0u32; 60];

    for i in 0..nk {
        w[i] = u32::from_be_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
    }

    for i in nk..nw {
        let mut temp = w[i - 1];
        if i % nk == 0 {
            temp = sub_word(rot_word(temp)) ^ ((RCON[i / nk - 1] as u32) << 24);
        } else if nk == 8 && i % nk == 4 {
            temp = sub_word(temp);
        }
        w[i] = w[i - nk] ^ temp;
    }

    for r in 0..=nr {
        let bytes = round_keys[r].as_mut();
        for j in 0..4 {
            let wb = w[r * 4 + j].to_be_bytes();
            bytes[j * 4..j * 4 + 4].copy_from_slice(&wb);
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn rot_word(w: u32) -> u32 {
    w.rotate_left(8)
}

#[cfg(target_arch = "aarch64")]
fn sub_word(w: u32) -> u32 {
    let b = w.to_be_bytes();
    u32::from_be_bytes([
        SBOX[b[0] as usize],
        SBOX[b[1] as usize],
        SBOX[b[2] as usize],
        SBOX[b[3] as usize],
    ])
}

#[cfg(target_arch = "aarch64")]
fn ctr_increment(ctr: &mut [u8; 16]) {
    for i in (0..16).rev() {
        ctr[i] = ctr[i].wrapping_add(1);
        if ctr[i] != 0 {
            break;
        }
    }
}

#[cfg(target_arch = "aarch64")]
static SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];
