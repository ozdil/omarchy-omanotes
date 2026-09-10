use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PBKDF2_ITERATIONS: u32 = 10_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedEnvelope {
    pub v: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

/// Computes HMAC-SHA256
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0x5cu8; 64];
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= k[i];
        i_key_pad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(&i_key_pad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&o_key_pad);
    outer.update(&inner_hash);
    outer.finalize().into()
}

/// Derives a 256-bit encryption key using PBKDF2-HMAC-SHA256 (RFC 2898)
pub fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut block_input = Vec::with_capacity(salt.len() + 4);
    block_input.extend_from_slice(salt);
    block_input.extend_from_slice(&1u32.to_be_bytes()); // block 1

    let mut u = hmac_sha256(password.as_bytes(), &block_input);
    let mut result = u;

    for _ in 1..PBKDF2_ITERATIONS {
        u = hmac_sha256(password.as_bytes(), &u);
        for i in 0..32 {
            result[i] ^= u[i];
        }
    }
    result
}

/// Generates a cryptographically secure random 16-byte salt
pub fn generate_salt() -> Result<[u8; SALT_LEN], String> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::getrandom(&mut salt).map_err(|e| format!("Failed to generate secure random salt: {}", e))?;
    Ok(salt)
}

/// Encrypts bytes using Zero-Knowledge AES-256-GCM
pub fn encrypt_payload(payload: &[u8], key: &[u8; 32], salt: &[u8]) -> Result<EncryptedEnvelope, String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| format!("Failed to generate random nonce: {}", e))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| format!("AES-256-GCM encryption failed: {}", e))?;

    Ok(EncryptedEnvelope {
        v: 1,
        salt: hex::encode(salt),
        nonce: hex::encode(nonce_bytes),
        ciphertext: hex::encode(ciphertext),
    })
}

/// Decrypts bytes from an EncryptedEnvelope using Zero-Knowledge AES-256-GCM
#[allow(dead_code)]
pub fn decrypt_payload(envelope: &EncryptedEnvelope, key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if envelope.v != 1 {
        return Err(format!("Unsupported envelope version: {}", envelope.v));
    }

    let nonce_bytes = hex::decode(&envelope.nonce).map_err(|e| format!("Invalid nonce hex: {}", e))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("Invalid nonce length".to_string());
    }

    let ciphertext_bytes = hex::decode(&envelope.ciphertext).map_err(|e| format!("Invalid ciphertext hex: {}", e))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext_bytes.as_slice())
        .map_err(|_| "AES-256-GCM decryption failed: invalid password or corrupted data".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption_roundtrip() {
        let password = "super_secret_master_password_42";
        let salt = generate_salt().expect("salt");
        let key = derive_key(password, &salt);

        let data = b"Hello, Omarchy E2EE Google Keep Notes!";
        let envelope = encrypt_payload(data, &key, &salt).expect("encryption");

        let decrypted = decrypt_payload(&envelope, &key).expect("decryption");
        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_wrong_password_fails_cleanly() {
        let salt = generate_salt().expect("salt");
        let key1 = derive_key("correct_password", &salt);
        let key2 = derive_key("wrong_password", &salt);

        let data = b"Confidential notes";
        let envelope = encrypt_payload(data, &key1, &salt).expect("encryption");

        let res = decrypt_payload(&envelope, &key2);
        assert!(res.is_err());
    }
}
