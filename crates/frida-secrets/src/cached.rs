//! An in-memory cache in front of a slow or prompting store (the OS keychain).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{SecretError, SecretKey, SecretStore};

/// What the inner store answered for one key.
#[derive(Debug, Clone)]
enum Cached {
    /// The value, or `None` when none is set.
    Value(Option<String>),
    /// The read failed (e.g. the user clicked "Deny" in the macOS prompt). Only the error text is
    /// kept; it never contains the secret.
    Failed(String),
}

/// Reads each secret from the inner store at most once per app run, and remembers failed reads.
///
/// Without it, every inspection (which runs while the user types) read every active secret from
/// the keychain. On macOS each read of an item the app isn't allowed to access yet shows a
/// password prompt, and an unsigned debug build loses "Always Allow" on every rebuild, so the
/// user got one prompt per secret per keystroke. Remembering a failure matters for the same
/// reason: after "Deny", the next keystroke would ask again.
///
/// `set` and `delete` go to the inner store and update the cache, so the cache can't go stale
/// through Frida. (A value changed outside Frida, e.g. in Keychain Access, shows up after a
/// restart.) Secret values stay in this process's memory only, like the values a send already
/// holds; they are never written anywhere.
///
/// The inner store is a `Box<dyn SecretStore>` (a trait object) rather than a generic `S`, to
/// keep the type simple: callers only ever see `CachedStore`.
pub struct CachedStore {
    inner: Box<dyn SecretStore>,
    /// `std::sync::Mutex` is fine here: it is only held for a map lookup or insert, never while
    /// waiting for the inner store (so one slow keychain read doesn't block other keys' hits).
    cache: Mutex<HashMap<SecretKey, Cached>>,
}

impl CachedStore {
    pub fn new(inner: Box<dyn SecretStore>) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn remember(&self, key: &SecretKey, entry: Cached) {
        self.cache
            .lock()
            .expect("secret cache lock")
            .insert(key.clone(), entry);
    }
}

impl SecretStore for CachedStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        let hit = self
            .cache
            .lock()
            .expect("secret cache lock")
            .get(key)
            .cloned();
        match hit {
            Some(Cached::Value(value)) => return Ok(value),
            Some(Cached::Failed(message)) => return Err(SecretError::Keychain(message)),
            None => {}
        }
        match self.inner.get(key) {
            Ok(value) => {
                self.remember(key, Cached::Value(value.clone()));
                Ok(value)
            }
            Err(err) => {
                self.remember(key, Cached::Failed(err.to_string()));
                Err(err)
            }
        }
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.inner.set(key, value)?;
        self.remember(key, Cached::Value(Some(value.to_string())));
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.inner.delete(key)?;
        self.remember(key, Cached::Value(None));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::MemoryStore;

    /// Counts reads and can fail them, like a keychain whose prompt was denied.
    #[derive(Default)]
    struct Counting {
        store: MemoryStore,
        reads: Arc<AtomicUsize>,
        deny: bool,
    }

    impl SecretStore for Counting {
        fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.deny {
                return Err(SecretError::Keychain("user denied access".into()));
            }
            self.store.get(key)
        }
        fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
            self.store.set(key, value)
        }
        fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
            self.store.delete(key)
        }
    }

    #[test]
    fn reads_each_secret_once_and_follows_set_and_delete() {
        let reads = Arc::new(AtomicUsize::new(0));
        let cached = CachedStore::new(Box::new(Counting {
            reads: Arc::clone(&reads),
            ..Counting::default()
        }));
        let key = SecretKey::new("c", "dev", "token");
        for _ in 0..5 {
            assert_eq!(cached.get(&key).unwrap(), None);
        }
        cached.set(&key, "s3cret").unwrap();
        assert_eq!(cached.get(&key).unwrap().as_deref(), Some("s3cret"));
        cached.delete(&key).unwrap();
        assert_eq!(cached.get(&key).unwrap(), None);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn remembers_a_denied_read() {
        let reads = Arc::new(AtomicUsize::new(0));
        let cached = CachedStore::new(Box::new(Counting {
            reads: Arc::clone(&reads),
            deny: true,
            ..Counting::default()
        }));
        let key = SecretKey::new("c", "dev", "token");
        for _ in 0..5 {
            assert!(cached.get(&key).is_err());
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
}
