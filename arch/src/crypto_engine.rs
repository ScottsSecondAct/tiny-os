#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CryptoHwError {
    NotAvailable,
    InvalidKeySize,
    InvalidInput,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AesMode {
    Ecb,
    Cbc,
    Ctr,
}

pub trait CryptoEngine {
    fn aes_encrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        input: &[u8],
        output: &mut [u8],
        mode: AesMode,
    ) -> Result<usize, CryptoHwError>;
    fn aes_decrypt(
        &self,
        key: &[u8],
        iv: &[u8],
        input: &[u8],
        output: &mut [u8],
        mode: AesMode,
    ) -> Result<usize, CryptoHwError>;
    fn sha256(&self, input: &[u8], output: &mut [u8; 32]) -> Result<(), CryptoHwError>;
    fn supported(&self) -> bool;
}
