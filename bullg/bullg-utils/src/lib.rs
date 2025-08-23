
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};

pub fn custom_encrypt(data: &[u8]) -> Vec<u8> {
    general_purpose::STANDARD.encode(data).into_bytes()
}
pub fn custom_decrypt(data: &[u8]) -> Result<Vec<u8>> {
    Ok(general_purpose::STANDARD.decode(data)?)
}