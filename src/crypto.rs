use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{bail, Context, Result};
use rand::RngCore;

/// AES-256-GCM helper used for OAuth credentials and channel API keys at rest.
pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    pub fn new(key: &[u8]) -> Result<Self> {
        if key.len() != 32 {
            bail!("加密密钥长度必须为 32 字节");
        }
        Ok(Self {
            cipher: Aes256Gcm::new_from_slice(key).context("初始化 AES-GCM 失败")?,
        })
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| anyhow::anyhow!("加密失败"))?;
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 13 {
            bail!("密文长度非法");
        }
        let (nonce, ciphertext) = data.split_at(12);
        self.cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow::anyhow!("解密失败"))
    }
}

pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.len() <= 4 {
        return "****".to_string();
    }
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("****{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_round_trip() {
        let key = [7u8; 32];
        let crypto = Crypto::new(&key).unwrap();
        let ciphertext = crypto.encrypt(b"sk-secret-channel-key").unwrap();
        assert_ne!(ciphertext, b"sk-secret-channel-key");
        assert_eq!(crypto.decrypt(&ciphertext).unwrap(), b"sk-secret-channel-key");
    }

    #[test]
    fn mask_keeps_tail() {
        assert_eq!(mask_secret("sk-1234567890abcd"), "****abcd");
        assert_eq!(mask_secret("abc"), "****");
    }
}
