use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PBKDF2_ITERATIONS: u32 = 10_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct KdfParams {
    pub algorithm: String,
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            algorithm: "argon2id".to_string(),
            m_cost: 65536, // 64 MiB (RFC 9106 recommended interactive/balanced memory cost)
            t_cost: 3,     // 3 iterations
            p_cost: 1,     // 1 lane
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EncryptedEnvelope {
    pub v: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kdf: Option<KdfParams>,
}

/// Computes HMAC-SHA256 (used for legacy v1 PBKDF2 compatibility)
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
    inner.update(i_key_pad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(o_key_pad);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// Derives a 256-bit encryption key using Argon2id with the provided KdfParams
pub fn derive_key_argon2id(
    password: &str,
    salt: &[u8],
    params: &KdfParams,
) -> Result<[u8; 32], String> {
    if params.algorithm.to_lowercase() != "argon2id" {
        return Err(format!("Unsupported KDF algorithm: {}", params.algorithm));
    }
    let argon2_params = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(32))
        .map_err(|e| format!("Invalid Argon2 parameters: {}", e))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Argon2id key derivation failed: {}", e))?;

    Ok(key)
}

/// Derives a 256-bit encryption key using legacy PBKDF2-HMAC-SHA256 (RFC 2898)
/// Maintained strictly for backward compatibility and migration of legacy v1 envelopes.
pub fn derive_key_pbkdf2(password: &str, salt: &[u8]) -> [u8; 32] {
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

/// Primary key derivation function utilizing calibrated Argon2id defaults (64 MiB, 3 passes)
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    derive_key_argon2id(password, salt, &KdfParams::default())
}

/// Generates a cryptographically secure random 16-byte salt
pub fn generate_salt() -> Result<[u8; SALT_LEN], String> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::getrandom(&mut salt)
        .map_err(|e| format!("Failed to generate secure random salt: {}", e))?;
    Ok(salt)
}

/// Encrypts bytes using Zero-Knowledge AES-256-GCM with version 2 Argon2id envelope
pub fn encrypt_payload(
    payload: &[u8],
    key: &[u8; 32],
    salt: &[u8],
) -> Result<EncryptedEnvelope, String> {
    encrypt_payload_with_kdf(payload, key, salt, Some(KdfParams::default()))
}

/// Encrypts bytes using Zero-Knowledge AES-256-GCM with explicit KdfParams
pub fn encrypt_payload_with_kdf(
    payload: &[u8],
    key: &[u8; 32],
    salt: &[u8],
    kdf: Option<KdfParams>,
) -> Result<EncryptedEnvelope, String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| format!("Failed to generate random nonce: {}", e))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| format!("AES-256-GCM encryption failed: {}", e))?;

    Ok(EncryptedEnvelope {
        v: 2,
        salt: hex::encode(salt),
        nonce: hex::encode(nonce_bytes),
        ciphertext: hex::encode(ciphertext),
        kdf: kdf.or_else(|| Some(KdfParams::default())),
    })
}

/// Decrypts bytes from an EncryptedEnvelope supporting both v2 (Argon2id) and v1 (legacy PBKDF2)
pub fn decrypt_envelope(envelope: &EncryptedEnvelope, password: &str) -> Result<Vec<u8>, String> {
    let salt_bytes = hex::decode(&envelope.salt).map_err(|e| format!("Invalid salt hex: {}", e))?;
    if salt_bytes.len() != SALT_LEN {
        return Err("Invalid salt length in envelope".to_string());
    }

    let key = match envelope.v {
        1 => {
            // Legacy v1 PBKDF2 migration path
            derive_key_pbkdf2(password, &salt_bytes)
        }
        2 => {
            // Modern v2 Argon2id
            let params = envelope.kdf.clone().unwrap_or_default();
            derive_key_argon2id(password, &salt_bytes, &params)?
        }
        other => return Err(format!("Unsupported envelope version: {}", other)),
    };

    decrypt_payload(envelope, &key)
}

/// Decrypts bytes from an EncryptedEnvelope using Zero-Knowledge AES-256-GCM given a 256-bit key
pub fn decrypt_payload(envelope: &EncryptedEnvelope, key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let nonce_bytes =
        hex::decode(&envelope.nonce).map_err(|e| format!("Invalid nonce hex: {}", e))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("Invalid nonce length".to_string());
    }

    let ciphertext_bytes =
        hex::decode(&envelope.ciphertext).map_err(|e| format!("Invalid ciphertext hex: {}", e))?;

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext_bytes.as_slice())
        .map_err(|_| {
            "AES-256-GCM decryption failed: invalid password or corrupted data".to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2id_encryption_decryption_roundtrip() {
        let password = "super_secret_master_password_42";
        let salt = generate_salt().expect("salt");
        let key = derive_key(password, &salt).expect("derive key");

        let data = b"Hello, Omarchy E2EE Google Keep Notes with Argon2id!";
        let envelope = encrypt_payload(data, &key, &salt).expect("encryption");
        assert_eq!(envelope.v, 2);
        assert!(envelope.kdf.is_some());
        assert_eq!(envelope.kdf.as_ref().unwrap().algorithm, "argon2id");

        let decrypted = decrypt_envelope(&envelope, password).expect("decryption via envelope");
        assert_eq!(data, decrypted.as_slice());
    }

    #[test]
    fn test_legacy_v1_pbkdf2_migration_to_v2() {
        let password = "legacy_v1_user_password";
        let salt = generate_salt().expect("salt");
        let legacy_key = derive_key_pbkdf2(password, &salt);

        // Construct a legacy v1 envelope without kdf field
        let data = b"Legacy note created with v1 PBKDF2";
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce_bytes).expect("nonce");
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&legacy_key));
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, data.as_ref()).expect("encrypt v1");

        let v1_envelope = EncryptedEnvelope {
            v: 1,
            salt: hex::encode(salt),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(ciphertext),
            kdf: None,
        };

        // Decrypt v1 envelope transparently
        let migrated_data = decrypt_envelope(&v1_envelope, password).expect("decrypt v1");
        assert_eq!(data, migrated_data.as_slice());

        // Re-encrypt to modern v2 Argon2id envelope
        let new_key = derive_key(password, &salt).expect("new argon2id key");
        let v2_envelope = encrypt_payload(&migrated_data, &new_key, &salt).expect("encrypt v2");
        assert_eq!(v2_envelope.v, 2);

        let v2_decrypted = decrypt_envelope(&v2_envelope, password).expect("decrypt v2");
        assert_eq!(data, v2_decrypted.as_slice());
    }

    #[test]
    fn test_wrong_password_fails_cleanly() {
        let salt = generate_salt().expect("salt");
        let key1 = derive_key("correct_password", &salt).expect("key1");

        let data = b"Confidential notes";
        let envelope = encrypt_payload(data, &key1, &salt).expect("encryption");

        let res = decrypt_envelope(&envelope, "wrong_password");
        assert!(res.is_err());
    }
}
