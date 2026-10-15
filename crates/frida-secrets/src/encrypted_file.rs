//! Fallback secret store: one encrypted file, unlocked with a master password (PLAN.md §6).
//!
//! The file holds a JSON map of `qualified key -> value`, encrypted as a whole with
//! XChaCha20-Poly1305. The key is derived from the master password with Argon2id and a random
//! salt. XChaCha's 24-byte nonce is large enough to pick at random on every save. Poly1305
//! authenticates the ciphertext, so a wrong password (or a tampered file) fails to decrypt
//! instead of producing garbage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::{SecretError, SecretKey, SecretStore};

const FILE_VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

/// What is written to disk. Everything except `version` is base64.
#[derive(Serialize, Deserialize)]
struct SecretFile {
    version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

/// The decrypted state while the store is unlocked.
struct Unlocked {
    key: [u8; KEY_LEN],
    salt: [u8; SALT_LEN],
    values: BTreeMap<String, String>,
}

pub struct EncryptedFileStore {
    path: PathBuf,
    /// `None` while locked. A `Mutex` because Tauri commands may use the store concurrently.
    unlocked: Mutex<Option<Unlocked>>,
}

impl EncryptedFileStore {
    /// A locked store for the file at `path` (which may not exist yet).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            unlocked: Mutex::new(None),
        }
    }

    /// Whether a secret file was already created (so `unlock` checks the password).
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked.lock().expect("secret file lock").is_some()
    }

    /// Unlocks the store. If the file doesn't exist yet, it is created with this password.
    pub fn unlock(&self, password: &str) -> Result<(), SecretError> {
        let unlocked = if self.path.exists() {
            self.open(password)?
        } else {
            let mut salt = [0u8; SALT_LEN];
            random(&mut salt)?;
            let unlocked = Unlocked {
                key: derive_key(password, &salt)?,
                salt,
                values: BTreeMap::new(),
            };
            self.write(&unlocked)?;
            unlocked
        };
        *self.unlocked.lock().expect("secret file lock") = Some(unlocked);
        Ok(())
    }

    /// Forgets the decrypted values and the key.
    pub fn lock(&self) {
        *self.unlocked.lock().expect("secret file lock") = None;
    }

    fn open(&self, password: &str) -> Result<Unlocked, SecretError> {
        let text = std::fs::read_to_string(&self.path).map_err(|e| self.file_error(e))?;
        let file: SecretFile = serde_json::from_str(&text).map_err(|e| self.file_error(e))?;
        let salt: [u8; SALT_LEN] = decode_exact(&file.salt).ok_or_else(|| self.corrupt())?;
        let nonce: [u8; NONCE_LEN] = decode_exact(&file.nonce).ok_or_else(|| self.corrupt())?;
        let ciphertext = BASE64
            .decode(&file.ciphertext)
            .map_err(|_| self.corrupt())?;

        let key = derive_key(password, &salt)?;
        let plaintext = cipher(&key)
            .decrypt(&XNonce::from(nonce), ciphertext.as_ref())
            .map_err(|_| SecretError::WrongPassword)?;
        let values = serde_json::from_slice(&plaintext).map_err(|_| self.corrupt())?;
        Ok(Unlocked { key, salt, values })
    }

    /// Encrypts and writes the values atomically (temp file + rename).
    fn write(&self, unlocked: &Unlocked) -> Result<(), SecretError> {
        let plaintext = serde_json::to_vec(&unlocked.values).map_err(|e| self.file_error(e))?;
        let mut nonce = [0u8; NONCE_LEN];
        random(&mut nonce)?;
        let ciphertext = cipher(&unlocked.key)
            .encrypt(&XNonce::from(nonce), plaintext.as_ref())
            .map_err(|_| self.corrupt())?;
        let file = SecretFile {
            version: FILE_VERSION,
            salt: BASE64.encode(unlocked.salt),
            nonce: BASE64.encode(nonce),
            ciphertext: BASE64.encode(ciphertext),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| self.file_error(e))?;

        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| self.file_error(e))?;
        let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| self.file_error(e))?;
        std::io::Write::write_all(&mut temp, text.as_bytes()).map_err(|e| self.file_error(e))?;
        temp.persist(&self.path)
            .map_err(|e| self.file_error(e.error))?;
        Ok(())
    }

    /// Runs `change` on the unlocked values and saves the result.
    fn modify(
        &self,
        change: impl FnOnce(&mut BTreeMap<String, String>),
    ) -> Result<(), SecretError> {
        let mut guard = self.unlocked.lock().expect("secret file lock");
        let unlocked = guard.as_mut().ok_or(SecretError::Locked)?;
        change(&mut unlocked.values);
        self.write(unlocked)
    }

    fn file_error(&self, e: impl std::fmt::Display) -> SecretError {
        SecretError::File {
            path: self.path.display().to_string(),
            message: e.to_string(),
        }
    }

    fn corrupt(&self) -> SecretError {
        self.file_error("the file is damaged")
    }
}

impl SecretStore for EncryptedFileStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        let guard = self.unlocked.lock().expect("secret file lock");
        let unlocked = guard.as_ref().ok_or(SecretError::Locked)?;
        Ok(unlocked.values.get(&key.qualified()).cloned())
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.modify(|values| {
            values.insert(key.qualified(), value.to_string());
        })
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.modify(|values| {
            values.remove(&key.qualified());
        })
    }
}

fn cipher(key: &[u8; KEY_LEN]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(&(*key).into())
}

/// Argon2id with the crate's recommended default parameters.
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], SecretError> {
    let mut key = [0u8; KEY_LEN];
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| SecretError::File {
            path: String::new(),
            message: format!("key derivation failed: {e}"),
        })?;
    Ok(key)
}

fn random(buf: &mut [u8]) -> Result<(), SecretError> {
    getrandom::fill(buf).map_err(|e| SecretError::File {
        path: String::new(),
        message: format!("no secure random numbers available: {e}"),
    })
}

/// Decodes base64 into a fixed-size array, or `None` if the length is wrong.
fn decode_exact<const N: usize>(text: &str) -> Option<[u8; N]> {
    BASE64.decode(text).ok()?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(var: &str) -> SecretKey {
        SecretKey::new("col-1", "dev", var)
    }

    #[test]
    fn values_survive_lock_and_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        let store = EncryptedFileStore::new(&path);

        store.unlock("hunter2").unwrap();
        store.set(&key("token"), "s3cr3t-value").unwrap();
        store.lock();
        assert!(matches!(store.get(&key("token")), Err(SecretError::Locked)));

        let reopened = EncryptedFileStore::new(&path);
        reopened.unlock("hunter2").unwrap();
        assert_eq!(
            reopened.get(&key("token")).unwrap().as_deref(),
            Some("s3cr3t-value")
        );
        assert_eq!(reopened.get(&key("missing")).unwrap(), None);
    }

    #[test]
    fn the_file_does_not_contain_the_plain_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        let store = EncryptedFileStore::new(&path);
        store.unlock("pw").unwrap();
        store.set(&key("token"), "s3cr3t-value").unwrap();

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("s3cr3t-value"));
        assert!(!on_disk.contains("token"));
    }

    #[test]
    fn wrong_password_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.enc");
        EncryptedFileStore::new(&path).unlock("right").unwrap();

        let store = EncryptedFileStore::new(&path);
        assert!(matches!(
            store.unlock("wrong"),
            Err(SecretError::WrongPassword)
        ));
        assert!(!store.is_unlocked());
    }

    #[test]
    fn delete_removes_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = EncryptedFileStore::new(dir.path().join("secrets.enc"));
        store.unlock("pw").unwrap();
        store.set(&key("a"), "1").unwrap();
        store.delete(&key("a")).unwrap();
        store.delete(&key("never-set")).unwrap();
        assert_eq!(store.get(&key("a")).unwrap(), None);
    }
}
