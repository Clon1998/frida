//! All secrets in **one** item of another store (PLAN.md M13, dev builds on macOS).
//!
//! An unsigned debug build can't use the data protection keychain, and the legacy keychain asks
//! for the password once per item. Keeping every secret in a single item turns that into one
//! prompt per app run.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::{SecretError, SecretKey, SecretStore};

/// Stores all secrets as one JSON object (qualified key → value) in a single item of `inner`.
///
/// The object is read once, on first use, and kept in memory; every change writes it back.
pub struct VaultStore {
    inner: Box<dyn SecretStore>,
    /// `None` until first loaded. Held while loading or saving, so two threads can't race
    /// (and the keychain is asked only once).
    values: Mutex<Option<BTreeMap<String, String>>>,
}

/// The one item in `inner` that holds everything: keychain account `frida/vault/secrets`.
fn vault_key() -> SecretKey {
    SecretKey::new("frida", "vault", "secrets")
}

impl VaultStore {
    pub fn new(inner: Box<dyn SecretStore>) -> Self {
        Self {
            inner,
            values: Mutex::new(None),
        }
    }

    /// Runs `work` on the loaded secrets; saves them afterwards when `work` returns `true`.
    fn with_values<T>(
        &self,
        work: impl FnOnce(&mut BTreeMap<String, String>) -> (T, bool),
    ) -> Result<T, SecretError> {
        let mut guard = self.values.lock().expect("vault lock");
        if guard.is_none() {
            let loaded = match self.inner.get(&vault_key())? {
                // A vault that can't be read is an error, never "empty": saving an empty one
                // over it would lose every secret.
                Some(json) => serde_json::from_str(&json).map_err(|e| {
                    SecretError::Keychain(format!("the secret vault is damaged: {e}"))
                })?,
                None => BTreeMap::new(),
            };
            *guard = Some(loaded);
        }
        let values = guard.as_mut().expect("loaded above");
        let (result, changed) = work(values);
        if changed {
            let json =
                serde_json::to_string(values).map_err(|e| SecretError::Keychain(e.to_string()))?;
            self.inner.set(&vault_key(), &json)?;
        }
        Ok(result)
    }
}

impl SecretStore for VaultStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        self.with_values(|v| (v.get(&key.qualified()).cloned(), false))
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.with_values(|v| {
            let previous = v.insert(key.qualified(), value.to_string());
            ((), previous.as_deref() != Some(value))
        })
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.with_values(|v| ((), v.remove(&key.qualified()).is_some()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::MemoryStore;

    /// Counts how often the single vault item is read and written.
    #[derive(Default)]
    struct Counting {
        store: MemoryStore,
        reads: Arc<AtomicUsize>,
        writes: Arc<AtomicUsize>,
    }

    impl SecretStore for Counting {
        fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.store.get(key)
        }
        fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.store.set(key, value)
        }
        fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.store.delete(key)
        }
    }

    #[test]
    fn all_secrets_live_in_one_item_read_once() {
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let vault = VaultStore::new(Box::new(Counting {
            reads: Arc::clone(&reads),
            writes: Arc::clone(&writes),
            ..Counting::default()
        }));
        let a = SecretKey::new("col", "dev", "token");
        let b = SecretKey::new("ws-1", "prod", "apiKey");
        assert_eq!(vault.get(&a).unwrap(), None);
        vault.set(&a, "one").unwrap();
        vault.set(&b, "two").unwrap();
        vault.set(&b, "two").unwrap(); // unchanged: no write
        assert_eq!(vault.get(&a).unwrap().as_deref(), Some("one"));
        vault.delete(&a).unwrap();
        assert_eq!(vault.get(&a).unwrap(), None);
        assert_eq!(vault.get(&b).unwrap().as_deref(), Some("two"));
        assert_eq!(
            reads.load(Ordering::SeqCst),
            1,
            "the vault item is read once"
        );
        assert_eq!(writes.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_new_run_reads_what_the_last_one_saved() {
        let shared = Arc::new(MemoryStore::default());
        struct Shared(Arc<MemoryStore>);
        impl SecretStore for Shared {
            fn get(&self, k: &SecretKey) -> Result<Option<String>, SecretError> {
                self.0.get(k)
            }
            fn set(&self, k: &SecretKey, v: &str) -> Result<(), SecretError> {
                self.0.set(k, v)
            }
            fn delete(&self, k: &SecretKey) -> Result<(), SecretError> {
                self.0.delete(k)
            }
        }
        let key = SecretKey::new("col", "dev", "token");
        VaultStore::new(Box::new(Shared(Arc::clone(&shared))))
            .set(&key, "kept")
            .unwrap();
        let next_run = VaultStore::new(Box::new(Shared(Arc::clone(&shared))));
        assert_eq!(next_run.get(&key).unwrap().as_deref(), Some("kept"));
    }

    #[test]
    fn a_damaged_vault_is_an_error_and_is_not_overwritten() {
        let inner = MemoryStore::default();
        inner.set(&vault_key(), "{ not json").unwrap();
        let vault = VaultStore::new(Box::new(inner));
        let key = SecretKey::new("col", "dev", "token");
        assert!(vault.get(&key).is_err());
        assert!(vault.set(&key, "x").is_err());
    }
}
