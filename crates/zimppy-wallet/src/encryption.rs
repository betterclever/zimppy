use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};
use rand::RngCore;

use crate::error::WalletError;

/// Magic header for encrypted wallet files: "ZMPENC1\0"
const MAGIC: &[u8; 8] = b"ZMPENC1\0";
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = 8 + SALT_LEN + NONCE_LEN; // 64 bytes

/// Argon2id parameters (OWASP minimum: 64MB, 3 iterations, 1 lane).
fn argon2_params() -> Params {
    Params::new(65536, 3, 1, Some(32)).expect("valid argon2 params")
}

fn derive_key(passphrase: &str, salt: &[u8; SALT_LEN]) -> Result<[u8; 32], WalletError> {
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params());
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| WalletError::Crypto(format!("argon2 failed: {e}")))?;
    Ok(key)
}

/// Encrypt plaintext wallet bytes with the given passphrase.
///
/// Output format:
/// ```text
/// [8 bytes magic] [32 bytes Argon2id salt] [24 bytes XChaCha20 nonce] [ciphertext+tag]
/// ```
pub fn encrypt(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, WalletError> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce_bytes);

    let key_bytes = derive_key(passphrase, &salt)?;
    let key = Key::from_slice(&key_bytes);
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| WalletError::Crypto(format!("encrypt failed: {e}")))?;

    let mut out = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt encrypted wallet bytes with the given passphrase.
pub fn decrypt(passphrase: &str, data: &[u8]) -> Result<Vec<u8>, WalletError> {
    if data.len() < HEADER_LEN + 16 {
        return Err(WalletError::Crypto("file too short to be encrypted".to_string()));
    }
    if &data[..8] != MAGIC {
        return Err(WalletError::Crypto("not an encrypted wallet file".to_string()));
    }

    let salt: &[u8; SALT_LEN] = data[8..8 + SALT_LEN].try_into().unwrap();
    let nonce_bytes: &[u8; NONCE_LEN] = data[8 + SALT_LEN..HEADER_LEN].try_into().unwrap();
    let ciphertext = &data[HEADER_LEN..];

    let key_bytes = derive_key(passphrase, salt)?;
    let key = Key::from_slice(&key_bytes);
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = XNonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| WalletError::Crypto("decryption failed — wrong passphrase?".to_string()))
}

/// Returns true if `data` starts with the encrypted wallet magic header.
pub fn is_encrypted(data: &[u8]) -> bool {
    data.len() >= 8 && &data[..8] == MAGIC
}
