//! The OS credential store, via the `keyring` crate.

use keyring::{Entry, Error as KeyringError};

use crate::{SecretError, SecretKey, SecretStore};

/// The keychain "service" every Frida secret is filed under. The account name is
/// `{scopeId}/{envName}/{varName}`, so the full key reads `frida/{scope}/{env}/{var}`.
const SERVICE: &str = "frida";

/// Whether an OS keychain can be used on this machine. When it can't (e.g. a Linux machine
/// without a Secret Service), the app falls back to [`crate::EncryptedFileStore`].
pub fn keychain_available() -> bool {
    Entry::store_status().is_ok()
}

#[derive(Debug, Default)]
pub struct KeychainStore;

impl KeychainStore {
    fn entry(key: &SecretKey) -> Result<Entry, SecretError> {
        let account = format!("{}/{}/{}", key.scope_id, key.env, key.var);
        Entry::new(SERVICE, &account).map_err(|e| match e {
            KeyringError::NoDefaultStore => SecretError::KeychainUnavailable(e.to_string()),
            other => SecretError::Keychain(other.to_string()),
        })
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        Self::entry(key)?
            .set_password(value)
            .map_err(|e| SecretError::Keychain(e.to_string()))
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(e) => Err(SecretError::Keychain(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real OS keychain (macOS may ask for permission), so it only runs on request:
    /// `cargo test -p frida-secrets -- --ignored`.
    #[test]
    #[ignore = "uses the real OS keychain"]
    fn round_trip_through_the_os_keychain() {
        let store = KeychainStore;
        let key = SecretKey::new("frida-test", "dev", "token");
        store.set(&key, "value-1").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("value-1"));
        store.delete(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
    }
}
