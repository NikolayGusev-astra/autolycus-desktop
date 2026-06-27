// src-tauri/src/secrets.rs
// Namespace-scoped wrapper over the OS keyring for Steersman's own secrets.
//
// All Steersman-owned credentials live under a single `SERVICE` namespace so
// they are easy to audit and do not collide with entries other apps may store
// under generic names. Every call is fallible-but-graceful: if the keyring is
// unavailable (headless host, locked keyring, missing DBus), the caller gets
// `Ok(None)` on read and a soft `Err` on write, never a panic.

/// Keyring service name. All Steersman secrets are stored under this.
pub const SERVICE: &str = "steersman-desktop";

/// Logical account names for the secrets Steersman owns.
pub mod account {
    /// Remote API key for the configured gateway/API server.
    pub const REMOTE_API_KEY: &str = "remote-api-key";
    /// Telegram bot token for notifications.
    pub const TELEGRAM_BOT_TOKEN: &str = "telegram-bot-token";
}

/// Store a secret under the Steersman namespace.
pub fn set(account: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(value)
        .map_err(|e| format!("Failed to store secret in keyring: {}", e))
}

/// Read a secret. Returns `Ok(None)` when no entry exists (expected on first
/// run, or after migration). Returns `Err` only on a genuine keyring failure.
pub fn get(account: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read secret from keyring: {}", e)),
    }
}

/// Delete a secret. Missing entry is treated as success (idempotent).
pub fn delete(account: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, account)
        .map_err(|e| format!("Keyring error: {}", e))?;
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete secret from keyring: {}", e)),
    }
}

/// Try to migrate a secret from a plaintext value into the keyring.
///
/// On success, returns `true` if a migration happened (value was non-empty and
/// stored), `false` if there was nothing to migrate. Errors are logged and
/// swallowed: a failed migration must not block startup — the plaintext copy
/// (protected by 0600) remains as a fallback.
pub fn migrate(account: &str, plaintext: &str) -> bool {
    if plaintext.is_empty() {
        return false;
    }
    // Don't overwrite an existing keyring entry — the user may have updated it
    // directly. Only migrate when the keyring has no value yet.
    if let Ok(Some(_)) = get(account) {
        return false;
    }
    match set(account, plaintext) {
        Ok(()) => {
            eprintln!(
                "[steersman] migrated '{}' secret from plaintext into the OS keyring",
                account
            );
            true
        }
        Err(e) => {
            eprintln!(
                "[steersman] warning: could not migrate '{}' into keyring (staying on plaintext): {}",
                account, e
            );
            false
        }
    }
}
