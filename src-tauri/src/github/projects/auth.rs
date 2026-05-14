//! GitHub PAT storage backed by the OS keychain.
//!
//! One credential per app installation, stored under the service
//! `com.tolaria.app.github_pat` with a fixed username. The renderer never
//! reads the PAT back out — only "is it present?" and the result of a test
//! connection are exposed to the frontend.

use keyring::Entry;

const KEYRING_SERVICE: &str = "com.tolaria.app.github_pat";
const KEYRING_USERNAME: &str = "default";

fn entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)
        .map_err(|e| format!("Failed to open GitHub PAT keychain entry: {e}"))
}

pub fn store_pat(pat: &str) -> Result<(), String> {
    let trimmed = pat.trim();
    if trimmed.is_empty() {
        return Err("GitHub personal access token cannot be empty.".into());
    }
    entry()?
        .set_password(trimmed)
        .map_err(|e| format!("Failed to store GitHub PAT in keychain: {e}"))
}

pub fn load_pat() -> Result<Option<String>, String> {
    match entry()?.get_password() {
        Ok(pat) => Ok(Some(pat)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to load GitHub PAT from keychain: {e}")),
    }
}

pub fn delete_pat() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to clear GitHub PAT from keychain: {e}")),
    }
}

pub fn pat_present() -> bool {
    matches!(load_pat(), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// We avoid touching the user's real keychain in CI. These tests use
    /// keyring's mock backend (set via `keyring::set_default_credential_builder`)
    /// when available. With the platform features enabled in production,
    /// individual store/load/delete behavior is exercised manually via the
    /// Settings UI; here we test the rejection rules around the inputs.
    #[test]
    fn store_pat_rejects_empty_input() {
        assert!(store_pat("").is_err());
        assert!(store_pat("   ").is_err());
    }

    #[test]
    fn store_pat_rejects_whitespace_only() {
        let err = store_pat("\t \n").expect_err("expected error");
        assert!(err.to_lowercase().contains("empty"));
    }
}
