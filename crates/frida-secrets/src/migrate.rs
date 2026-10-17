//! Moving secrets from the legacy per-item keychain entries to a new store (PLAN.md M13).

use crate::{SecretError, SecretKey, SecretStore};

/// Reads from `current`; a secret found only in `legacy` is copied to `current` and deleted
/// from `legacy` on first use. Every legacy item is therefore read once more (on macOS: one
/// last password prompt for it) and never again.
pub struct MigratingStore {
    current: Box<dyn SecretStore>,
    legacy: Box<dyn SecretStore>,
}

impl MigratingStore {
    pub fn new(current: Box<dyn SecretStore>, legacy: Box<dyn SecretStore>) -> Self {
        Self { current, legacy }
    }
}

impl SecretStore for MigratingStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        if let Some(value) = self.current.get(key)? {
            return Ok(Some(value));
        }
        let Some(value) = self.legacy.get(key)? else {
            return Ok(None);
        };
        self.current.set(key, &value)?;
        // Already safe in `current`; a failed cleanup only leaves a stale copy behind.
        if let Err(err) = self.legacy.delete(key) {
            tracing::warn!(key = %key.qualified(), %err, "could not delete a migrated legacy secret");
        }
        tracing::info!(key = %key.qualified(), "secret moved to the new keychain");
        Ok(Some(value))
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.current.set(key, value)
    }

    /// Deletes in both, so a deleted secret can't come back from a legacy copy.
    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.current.delete(key)?;
        self.legacy.delete(key)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::MemoryStore;

    /// Shares one `MemoryStore` between the test and the `MigratingStore`.
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

    fn setup() -> (MigratingStore, Arc<MemoryStore>, Arc<MemoryStore>) {
        let current = Arc::new(MemoryStore::default());
        let legacy = Arc::new(MemoryStore::default());
        let store = MigratingStore::new(
            Box::new(Shared(Arc::clone(&current))),
            Box::new(Shared(Arc::clone(&legacy))),
        );
        (store, current, legacy)
    }

    #[test]
    fn a_legacy_secret_moves_over_on_first_read() {
        let (store, current, legacy) = setup();
        let key = SecretKey::new("col", "dev", "token");
        legacy.set(&key, "old").unwrap();

        assert_eq!(store.get(&key).unwrap().as_deref(), Some("old"));
        assert_eq!(current.get(&key).unwrap().as_deref(), Some("old"));
        assert_eq!(legacy.get(&key).unwrap(), None, "the legacy item is gone");
        // The new store wins from now on.
        current.set(&key, "new").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("new"));
    }

    #[test]
    fn deleting_also_removes_a_legacy_copy() {
        let (store, current, legacy) = setup();
        let key = SecretKey::new("col", "dev", "token");
        legacy.set(&key, "old").unwrap();
        current.set(&key, "new").unwrap();
        store.delete(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
        assert_eq!(legacy.get(&key).unwrap(), None);
    }

    #[test]
    fn missing_everywhere_is_none() {
        let (store, _, _) = setup();
        assert_eq!(store.get(&SecretKey::new("a", "b", "c")).unwrap(), None);
    }

    /// The macOS setup of a signed build: protected keychain ← (vault ← old per-secret items).
    /// Secrets from both a dev build's vault and old items end up in the protected keychain.
    #[test]
    fn a_signed_build_takes_over_the_vault_and_old_items() {
        use crate::VaultStore;
        let protected = Arc::new(MemoryStore::default());
        let login_keychain = Arc::new(MemoryStore::default());
        let old = SecretKey::new("col", "dev", "fromOldItem");
        let dev = SecretKey::new("col", "dev", "fromDevVault");
        login_keychain.set(&old, "a").unwrap();
        // What a dev build left behind: a secret in the vault item.
        VaultStore::new(Box::new(Shared(Arc::clone(&login_keychain))))
            .set(&dev, "b")
            .unwrap();

        let vault = MigratingStore::new(
            Box::new(VaultStore::new(Box::new(Shared(Arc::clone(
                &login_keychain,
            ))))),
            Box::new(Shared(Arc::clone(&login_keychain))),
        );
        let signed = MigratingStore::new(Box::new(Shared(Arc::clone(&protected))), Box::new(vault));
        assert_eq!(signed.get(&old).unwrap().as_deref(), Some("a"));
        assert_eq!(signed.get(&dev).unwrap().as_deref(), Some("b"));
        assert_eq!(protected.get(&old).unwrap().as_deref(), Some("a"));
        assert_eq!(protected.get(&dev).unwrap().as_deref(), Some("b"));
        // Nothing is left behind in the login keychain except the (now empty) vault item.
        assert_eq!(login_keychain.get(&old).unwrap(), None);
        let rest = VaultStore::new(Box::new(Shared(Arc::clone(&login_keychain))));
        assert_eq!(rest.get(&dev).unwrap(), None);
    }
}
