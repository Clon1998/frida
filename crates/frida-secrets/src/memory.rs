use std::collections::HashMap;
use std::sync::Mutex;

use crate::{SecretError, SecretKey, SecretStore};

/// Keeps secrets in memory only. Used in tests.
#[derive(Debug, Default)]
pub struct MemoryStore {
    values: Mutex<HashMap<SecretKey, String>>,
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        Ok(self
            .values
            .lock()
            .expect("memory store lock")
            .get(key)
            .cloned())
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .expect("memory store lock")
            .insert(key.clone(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        self.values.lock().expect("memory store lock").remove(key);
        Ok(())
    }
}
