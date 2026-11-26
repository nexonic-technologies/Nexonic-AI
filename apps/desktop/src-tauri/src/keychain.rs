//! macOS Keychain storage for the memory database encryption key.
//!
//! Keeps the 256-bit XChaCha20-Poly1305 key inside the user's Keychain rather
//! than on disk. Never falls back to an unencrypted or hardcoded key.

use nexonic_memory_engine::persist::generate_key;
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
const SERVICE: &str = "com.nexonictechnologies.ai";
#[cfg(target_os = "macos")]
const ACCOUNT: &str = "memory-db-key";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn encode_hex(key: &[u8; 32]) -> Zeroizing<String> {
    let mut s = Zeroizing::new(String::with_capacity(64));
    for b in key {
        use std::fmt::Write;
        let _ = write!(&mut *s, "{b:02x}");
    }
    s
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn decode_hex(raw: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    let trimmed = std::str::from_utf8(raw)
        .map_err(|_| "Keychain memory key is not valid UTF-8".to_string())?
        .trim();
    if trimmed.len() != 64 {
        return Err(format!(
            "Keychain memory key has unexpected length {} (expected 64 hex chars)",
            trimmed.len()
        ));
    }
    let mut out = Zeroizing::new([0u8; 32]);
    let bytes = trimmed.as_bytes();
    for i in 0..32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("Keychain memory key contains non-hex characters".to_string()),
    }
}

#[cfg(target_os = "macos")]
pub fn load_or_create_key() -> Result<Zeroizing<[u8; 32]>, String> {
    use security_framework::passwords::{get_generic_password, set_generic_password};

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => {
            let secret = Zeroizing::new(bytes);
            decode_hex(&secret)
        }
        Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => {
            let key = generate_key();
            let hex = encode_hex(&key);
            set_generic_password(SERVICE, ACCOUNT, hex.as_bytes())
                .map_err(|e| format!("failed to save memory key in macOS Keychain: {e}"))?;
            Ok(key)
        }
        Err(err) => Err(format!(
            "failed to read memory key from macOS Keychain: {err}"
        )),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn load_or_create_key() -> Result<Zeroizing<[u8; 32]>, String> {
    let _ = generate_key();
    Err("Keychain memory key storage is currently implemented for macOS only".into())
}

#[cfg(target_os = "macos")]
fn provider_account(provider: &str) -> String {
    format!("provider-key-{provider}")
}

pub fn mask_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() <= 10 {
        return "••••".into();
    }
    let prefix: String = trimmed.chars().take(6).collect();
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}…{suffix}")
}

#[cfg(target_os = "macos")]
pub fn load_provider_key(provider: &str) -> Result<Option<Zeroizing<String>>, String> {
    use security_framework::passwords::get_generic_password;

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    let account = provider_account(provider);
    match get_generic_password(SERVICE, &account) {
        Ok(bytes) => {
            let secret = Zeroizing::new(bytes);
            let text = std::str::from_utf8(&secret)
                .map_err(|_| "Keychain provider key is not valid UTF-8".to_string())?
                .trim()
                .to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(Zeroizing::new(text)))
            }
        }
        Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(err) => Err(format!("failed to read provider key from Keychain: {err}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn load_provider_key(_provider: &str) -> Result<Option<Zeroizing<String>>, String> {
    Ok(None)
}

#[cfg(target_os = "macos")]
pub fn save_provider_key(provider: &str, api_key: &str) -> Result<(), String> {
    use security_framework::passwords::set_generic_password;

    let cleaned = api_key.trim();
    if cleaned.is_empty() {
        return Err("API key must not be empty".into());
    }
    let account = provider_account(provider);
    set_generic_password(SERVICE, &account, cleaned.as_bytes())
        .map_err(|e| format!("failed to save provider key in macOS Keychain: {e}"))
}

#[cfg(not(target_os = "macos"))]
pub fn save_provider_key(_provider: &str, _api_key: &str) -> Result<(), String> {
    Err("Keychain provider key storage is currently implemented for macOS only".into())
}

#[cfg(target_os = "macos")]
pub fn delete_provider_key(provider: &str) -> Result<(), String> {
    use security_framework::passwords::delete_generic_password;

    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    let account = provider_account(provider);
    match delete_generic_password(SERVICE, &account) {
        Ok(()) => Ok(()),
        Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(err) => Err(format!("failed to clear provider key from Keychain: {err}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn delete_provider_key(_provider: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_matches_key() {
        let key = generate_key();
        let hex = encode_hex(&key);
        assert_eq!(hex.len(), 64);
        let decoded = decode_hex(hex.as_bytes()).unwrap();
        assert_eq!(*decoded, *key);
    }

    #[test]
    fn invalid_hex_is_rejected() {
        assert!(decode_hex(b"short").is_err());
        assert!(decode_hex(&[b'z'; 64]).is_err());
    }

    #[test]
    fn provider_key_mask_hides_middle() {
        assert_eq!(mask_key("sk-or-v1-1234567890abcdef"), "sk-or-…cdef");
        assert_eq!(mask_key("short"), "••••");
    }
}
