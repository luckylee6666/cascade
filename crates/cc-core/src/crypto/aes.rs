use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use rand::rngs::OsRng;
use rand::RngCore;

use super::envelope::{wrap_envelope, unwrap_envelope};

pub fn encrypt(plaintext: &str, master_key: &[u8; 32]) -> Result<String> {
    let cipher = Aes256Gcm::new_from_slice(master_key)
        .map_err(|e| anyhow::anyhow!("Invalid key length: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    Ok(wrap_envelope(&ciphertext, &nonce_bytes))
}

pub fn decrypt(envelope: &str, master_key: &[u8; 32]) -> Result<String> {
    let (nonce_bytes, ciphertext) = unwrap_envelope(envelope)?;

    let cipher = Aes256Gcm::new_from_slice(master_key)
        .map_err(|e| anyhow::anyhow!("Invalid key length: {}", e))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(String::from_utf8(plaintext)?)
}

pub fn get_or_create_master_key() -> Result<[u8; 32]> {
    // 1. Try environment variable (new name first, legacy second)
    if let Ok(key_str) = std::env::var("CASCADE_MASTER_KEY")
        .or_else(|_| std::env::var("CC_MASTER_KEY"))
    {
        use base64::Engine;
        let key_bytes = base64::engine::general_purpose::STANDARD.decode(&key_str)?;
        if key_bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_bytes);
            return Ok(key);
        }
    }

    // 2. Try keyring
    if let Ok(key) = try_keyring() {
        return Ok(key);
    }

    // 3. Try file
    if let Ok(key) = try_file_key() {
        return Ok(key);
    }

    // 4. Generate new key
    let mut key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut OsRng, &mut key);
    save_key_to_file(&key)?;

    Ok(key)
}

/// `~/.cascade/master.key`, falling back to the legacy `~/.cc/master.key`.
fn key_path(read_only: bool) -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("No home dir"))?;
    let new_path = home.join(".cascade").join("master.key");
    if read_only && !new_path.exists() {
        let legacy = home.join(".cc").join("master.key");
        if legacy.exists() {
            return Ok(legacy);
        }
    }
    Ok(new_path)
}

fn try_keyring() -> Result<[u8; 32]> {
    let entry = keyring::Entry::new("cascade", "master-key")
        .or_else(|_| keyring::Entry::new("config-center", "master-key"))?;
    let key_str = entry.get_password()?;
    use base64::Engine;
    let key_bytes = base64::engine::general_purpose::STANDARD.decode(&key_str)?;
    if key_bytes.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        return Ok(key);
    }
    anyhow::bail!("Invalid key length in keyring")
}

fn try_file_key() -> Result<[u8; 32]> {
    let key_path = key_path(true)?;

    if !key_path.exists() {
        anyhow::bail!("No key file");
    }

    let key_str = std::fs::read_to_string(&key_path)?;
    use base64::Engine;
    let key_bytes = base64::engine::general_purpose::STANDARD.decode(key_str.trim())?;
    if key_bytes.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        return Ok(key);
    }
    anyhow::bail!("Invalid key length in file")
}

fn save_key_to_file(key: &[u8; 32]) -> Result<()> {
    use base64::Engine;
    let key_path = key_path(false)?;

    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&key_path, base64::engine::general_purpose::STANDARD.encode(key))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = [42u8; 32];
        let plaintext = "hello world";

        let encrypted = encrypt(plaintext, &key).unwrap();
        assert!(encrypted.starts_with("cc-enc:v1:"));

        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_ciphertexts() {
        let key = [42u8; 32];
        let plaintext = "same text";

        let enc1 = encrypt(plaintext, &key).unwrap();
        let enc2 = encrypt(plaintext, &key).unwrap();

        // Different nonces -> different ciphertexts
        assert_ne!(enc1, enc2);

        // Both decrypt to same plaintext
        assert_eq!(decrypt(&enc1, &key).unwrap(), plaintext);
        assert_eq!(decrypt(&enc2, &key).unwrap(), plaintext);
    }
}
