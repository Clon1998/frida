//! An in-memory cache in front of a slow or prompting store (the OS keychain).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{SecretError, SecretKey, SecretStore};

/// What the inner store answered for one key.
#[derive(Debug, Clone)]
enum Cached {
    /// The value, or `None` when none is set.
    Value(Option<String>),
    /// The read failed (e.g. the user clicked "Deny" in the macOS prompt). Errors never contain
    /// the secret.
    Failed(SecretError),
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
/// Reads of the inner store happen one at a time (`reading`). A keychain read blocks while its
/// macOS prompt is open, and meanwhile every keystroke starts another inspection; without this,
/// each of those found the cache still empty and opened a prompt of its own. Now they wait, and
/// then find the first read's answer in the cache.
///
/// The inner store is a `Box<dyn SecretStore>` (a trait object) rather than a generic `S`, to
/// keep the type simple: callers only ever see `CachedStore`.
pub struct CachedStore {
    inner: Box<dyn SecretStore>,
    /// Only held for a map lookup or insert, so cache hits never wait for a slow read.
    cache: Mutex<HashMap<SecretKey, Cached>>,
    /// Held during a read of the inner store. Blocking other threads on it is fine: all callers
    /// already run in `spawn_blocking` (it's a `std` mutex, never held across an `.await`).
    reading: Mutex<()>,
}

impl CachedStore {
    pub fn new(inner: Box<dyn SecretStore>) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
            reading: Mutex::new(()),
        }
    }

    fn lookup(&self, key: &SecretKey) -> Option<Result<Option<String>, SecretError>> {
        match self.cache.lock().expect("secret cache lock").get(key)? {
            Cached::Value(value) => Some(Ok(value.clone())),
            Cached::Failed(err) => Some(Err(err.clone())),
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
        if let Some(hit) = self.lookup(key) {
            return hit;
        }
        // `_reading` is a guard: the lock is released when it goes out of scope (RAII).
        let _reading = self.reading.lock().expect("secret read lock");
        // Another thread may have read this key while we waited for the lock.
        if let Some(hit) = self.lookup(key) {
            return hit;
        }
        match self.inner.get(key) {
            Ok(value) => {
                self.remember(key, Cached::Value(value.clone()));
                Ok(value)
            }
            Err(err) => {
                self.remember(key, Cached::Failed(err.clone()));
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

    /// Like a keychain read waiting for the user to answer the macOS prompt.
    struct Slow(Arc<AtomicUsize>);

    impl SecretStore for Slow {
        fn get(&self, _key: &SecretKey) -> Result<Option<String>, SecretError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(100));
            Ok(Some("v".into()))
        }
        fn set(&self, _key: &SecretKey, _value: &str) -> Result<(), SecretError> {
            Ok(())
        }
        fn delete(&self, _key: &SecretKey) -> Result<(), SecretError> {
            Ok(())
        }
    }

    #[test]
    fn concurrent_reads_of_an_uncached_secret_ask_the_store_once() {
        let reads = Arc::new(AtomicUsize::new(0));
        let cached = Arc::new(CachedStore::new(Box::new(Slow(Arc::clone(&reads)))));
        let key = SecretKey::new("c", "dev", "token");
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let cached = Arc::clone(&cached);
                let key = key.clone();
                std::thread::spawn(move || cached.get(&key).unwrap())
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap().as_deref(), Some("v"));
        }
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
            let err = cached.get(&key).unwrap_err();
            assert_eq!(err.to_string(), "keychain error: user denied access");
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
}
