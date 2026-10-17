//! The macOS **data protection** keychain (PLAN.md M13): the iOS-style keychain. Its items belong
//! to the app's access group, so a signed Frida reads its own secrets without any password
//! prompt (unlike the legacy file-based keychain, which asks per item).
//!
//! It needs a signed app whose entitlements (`keychain-access-groups`) are backed by an embedded
//! provisioning profile; [`ProtectedStore::available`] checks that at startup.

use security_framework::passwords::{
    delete_generic_password_options, generic_password, set_generic_password_options,
};
use security_framework::passwords_options::PasswordOptions;

use crate::{SecretError, SecretKey, SecretStore};

/// `errSecItemNotFound`: no item for this key.
const NOT_FOUND: i32 = -25300;

/// The keychain "service" of every Frida secret (the account is `{scope}/{env}/{var}`).
const SERVICE: &str = "frida";

pub struct ProtectedStore {
    /// e.g. `Q4XB68D4T7.io.github.clon1998.frida` (team id + bundle id).
    access_group: String,
}

impl ProtectedStore {
    pub fn new(access_group: impl Into<String>) -> Self {
        Self {
            access_group: access_group.into(),
        }
    }

    /// Whether this process may use the data protection keychain: a read of a key that doesn't
    /// exist answers "not found" when it may, and `errSecMissingEntitlement` (or another error)
    /// when it isn't signed with the entitlement, e.g. in `tauri dev`.
    pub fn available(&self) -> bool {
        let probe = SecretKey::new("frida", "probe", "probe");
        match generic_password(self.options(&probe)) {
            Ok(_) => true,
            Err(err) if err.code() == NOT_FOUND => true,
            Err(err) => {
                tracing::info!(code = err.code(), "data protection keychain not available");
                false
            }
        }
    }

    fn options(&self, key: &SecretKey) -> PasswordOptions {
        let account = format!("{}/{}/{}", key.scope_id, key.env, key.var);
        let mut options = PasswordOptions::new_generic_password(SERVICE, &account);
        options.use_protected_keychain();
        options.set_access_group(&self.access_group);
        options
    }
}

fn keychain_error(err: security_framework::base::Error) -> SecretError {
    SecretError::Keychain(err.to_string())
}

impl SecretStore for ProtectedStore {
    fn get(&self, key: &SecretKey) -> Result<Option<String>, SecretError> {
        match generic_password(self.options(key)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| SecretError::Keychain("a stored secret is not valid text".into())),
            Err(err) if err.code() == NOT_FOUND => Ok(None),
            Err(err) => Err(keychain_error(err)),
        }
    }

    fn set(&self, key: &SecretKey, value: &str) -> Result<(), SecretError> {
        // Adds the item, or updates it if it exists.
        set_generic_password_options(value.as_bytes(), self.options(key)).map_err(keychain_error)
    }

    fn delete(&self, key: &SecretKey) -> Result<(), SecretError> {
        match delete_generic_password_options(self.options(key)) {
            Ok(()) => Ok(()),
            Err(err) if err.code() == NOT_FOUND => Ok(()),
            Err(err) => Err(keychain_error(err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test binary isn't signed with the entitlement, so the protected keychain must report
    /// itself unavailable (and Frida falls back to the vault) instead of failing later.
    #[test]
    fn an_unsigned_binary_cannot_use_the_protected_keychain() {
        assert!(!ProtectedStore::new("Q4XB68D4T7.io.github.clon1998.frida").available());
    }
}
