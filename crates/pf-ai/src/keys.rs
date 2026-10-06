//! Where API keys live: the OS credential store (macOS Keychain, Windows Credential Manager,
//! the Secret Service on Linux), one entry per provider under the app's identifier. When there is
//! no credential store, a key can be kept for this session only, in memory, never on disk.

use crate::error::AiError;
use crate::provider::ProviderId;
use crate::secret::ApiKey;
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

/// The credential store service name: the app's identifier.
pub const SERVICE: &str = "com.bradh11.pixelflow";

/// Why the credential store couldn't be used. Never carries platform details: some of them hold
/// the stored bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// There's no credential store on this computer (Linux without a Secret Service, say).
    Unavailable,
    /// It's there but locked, or PixelFlow was refused access.
    Denied,
    /// Anything else.
    Failed,
}

/// Saves, reads, and deletes one secret per account. Tests use [`MemoryStore`].
pub trait CredentialStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<ApiKey>, StoreError>;
    fn set(&self, account: &str, key: &ApiKey) -> Result<(), StoreError>;
    fn delete(&self, account: &str) -> Result<(), StoreError>;
    /// Whether keys can be saved here at all.
    fn available(&self) -> bool;
}

/// The OS credential store, through the `keyring` crate.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsKeychain;

impl OsKeychain {
    fn entry(account: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(SERVICE, account).map_err(store_error)
    }
}

/// Maps a keyring error to ours without keeping (or printing) anything inside it.
fn store_error(error: keyring::Error) -> StoreError {
    match error {
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => StoreError::Unavailable,
        keyring::Error::NoStorageAccess(_) => StoreError::Denied,
        _ => StoreError::Failed,
    }
}

impl CredentialStore for OsKeychain {
    fn get(&self, account: &str) -> Result<Option<ApiKey>, StoreError> {
        match Self::entry(account)?.get_password() {
            Ok(text) => Ok(ApiKey::new(zeroize::Zeroizing::new(text).as_str()).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(store_error(error)),
        }
    }

    fn set(&self, account: &str, key: &ApiKey) -> Result<(), StoreError> {
        Self::entry(account)?
            .set_password(key.expose())
            .map_err(store_error)
    }

    fn delete(&self, account: &str) -> Result<(), StoreError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(store_error(error)),
        }
    }

    fn available(&self) -> bool {
        keyring::Entry::store_status().is_ok()
    }
}

/// An in-memory credential store, for tests (and a stand-in where there's no real one).
#[derive(Debug, Default)]
pub struct MemoryStore {
    keys: Mutex<HashMap<String, ApiKey>>,
    unavailable: bool,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that behaves like a computer with no credential store at all.
    pub fn unavailable() -> Self {
        Self {
            keys: Mutex::default(),
            unavailable: true,
        }
    }

    fn keys(&self) -> std::sync::MutexGuard<'_, HashMap<String, ApiKey>> {
        self.keys.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The accounts holding a key (for tests).
    pub fn accounts(&self) -> Vec<String> {
        let mut accounts: Vec<String> = self.keys().keys().cloned().collect();
        accounts.sort();
        accounts
    }
}

impl CredentialStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<ApiKey>, StoreError> {
        if self.unavailable {
            return Err(StoreError::Unavailable);
        }
        Ok(self.keys().get(account).cloned())
    }

    fn set(&self, account: &str, key: &ApiKey) -> Result<(), StoreError> {
        if self.unavailable {
            return Err(StoreError::Unavailable);
        }
        self.keys().insert(account.to_string(), key.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), StoreError> {
        if self.unavailable {
            return Err(StoreError::Unavailable);
        }
        self.keys().remove(account);
        Ok(())
    }

    fn available(&self) -> bool {
        !self.unavailable
    }
}

/// What this platform calls its credential store, for messages.
pub fn keychain_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Keychain"
    } else if cfg!(target_os = "windows") {
        "Windows Credential Manager"
    } else {
        "system keyring"
    }
}

/// Where a provider's key is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyLocation {
    /// In the OS credential store: kept across runs.
    Keychain,
    /// In memory for this run only (no credential store, or the user chose so).
    Session,
}

/// The keys PixelFlow knows: the credential store, plus keys kept for this session only.
/// Keys go in and are used for requests; nothing here ever hands one back to the window.
pub struct KeyVault {
    store: Box<dyn CredentialStore>,
    session: Mutex<HashMap<ProviderId, ApiKey>>,
    /// Keys read from the store this run, so each chat turn doesn't go back to it.
    cache: Mutex<HashMap<ProviderId, ApiKey>>,
}

impl std::fmt::Debug for KeyVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeyVault(<redacted>)")
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl KeyVault {
    pub fn new(store: Box<dyn CredentialStore>) -> Self {
        Self {
            store,
            session: Mutex::default(),
            cache: Mutex::default(),
        }
    }

    /// The OS credential store.
    pub fn os() -> Self {
        Self::new(Box::new(OsKeychain))
    }

    /// Whether keys can be saved across runs on this computer.
    pub fn keychain_available(&self) -> bool {
        self.store.available()
    }

    /// Saves the key in the credential store (replacing any earlier one, and any session key).
    /// With no credential store, says so plainly: the UI then offers [`Self::use_for_session`].
    pub fn save(&self, provider: ProviderId, key: ApiKey) -> Result<KeyLocation, AiError> {
        self.store
            .set(provider.account(), &key)
            .map_err(|e| store_message(e, "save"))?;
        lock(&self.session).remove(&provider);
        lock(&self.cache).insert(provider, key);
        Ok(KeyLocation::Keychain)
    }

    /// Keeps the key in memory until PixelFlow quits. Never written anywhere.
    pub fn use_for_session(&self, provider: ProviderId, key: ApiKey) -> KeyLocation {
        lock(&self.session).insert(provider, key);
        KeyLocation::Session
    }

    /// Where the provider's key is, if there is one.
    pub fn location(&self, provider: ProviderId) -> Result<Option<KeyLocation>, AiError> {
        if lock(&self.session).contains_key(&provider) {
            return Ok(Some(KeyLocation::Session));
        }
        Ok(self.stored(provider)?.map(|_| KeyLocation::Keychain))
    }

    pub fn has(&self, provider: ProviderId) -> Result<bool, AiError> {
        Ok(self.location(provider)?.is_some())
    }

    /// Forgets the key everywhere: the session and the credential store.
    pub fn remove(&self, provider: ProviderId) -> Result<(), AiError> {
        lock(&self.session).remove(&provider);
        lock(&self.cache).remove(&provider);
        match self.store.delete(provider.account()) {
            Ok(()) | Err(StoreError::Unavailable) => Ok(()),
            Err(e) => Err(store_message(e, "remove")),
        }
    }

    /// The key to send with a request (session key first).
    pub fn key(&self, provider: ProviderId) -> Result<ApiKey, AiError> {
        if let Some(key) = lock(&self.session).get(&provider) {
            return Ok(key.clone());
        }
        self.stored(provider)?.ok_or(AiError::NoKey(provider))
    }

    fn stored(&self, provider: ProviderId) -> Result<Option<ApiKey>, AiError> {
        if let Some(key) = lock(&self.cache).get(&provider) {
            return Ok(Some(key.clone()));
        }
        let key = match self.store.get(provider.account()) {
            Ok(key) => key,
            Err(StoreError::Unavailable) => None,
            Err(e) => return Err(store_message(e, "read")),
        };
        if let Some(key) = &key {
            lock(&self.cache).insert(provider, key.clone());
        }
        Ok(key)
    }
}

fn store_message(error: StoreError, action: &str) -> AiError {
    let name = keychain_name();
    AiError::KeyStore(match error {
        StoreError::Unavailable => format!(
            "This computer has no {name} PixelFlow can use, so the key can't be saved. You can use it for this session only."
        ),
        StoreError::Denied => format!(
            "PixelFlow couldn't {action} the key: the {name} is locked or access was refused. Unlock it and try again."
        ),
        StoreError::Failed => format!("PixelFlow couldn't {action} the key in the {name}. Try again."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const FAKE: &str = "sk-test-not-a-key";

    /// Shares one memory store between the vault and the test.
    struct Shared(Arc<MemoryStore>);
    impl CredentialStore for Shared {
        fn get(&self, a: &str) -> Result<Option<ApiKey>, StoreError> {
            self.0.get(a)
        }
        fn set(&self, a: &str, k: &ApiKey) -> Result<(), StoreError> {
            self.0.set(a, k)
        }
        fn delete(&self, a: &str) -> Result<(), StoreError> {
            self.0.delete(a)
        }
        fn available(&self) -> bool {
            self.0.available()
        }
    }

    #[test]
    fn keys_are_saved_one_entry_per_provider_and_removed() {
        let store = Arc::new(MemoryStore::new());
        let vault = KeyVault::new(Box::new(Shared(store.clone())));
        assert!(!vault.has(ProviderId::Anthropic).unwrap());
        assert_eq!(
            vault.key(ProviderId::Openai).unwrap_err(),
            AiError::NoKey(ProviderId::Openai)
        );

        let saved = vault
            .save(ProviderId::Anthropic, ApiKey::new(FAKE).unwrap())
            .unwrap();
        assert_eq!(saved, KeyLocation::Keychain);
        assert_eq!(store.accounts(), ["anthropic"]);
        assert_eq!(
            vault.location(ProviderId::Anthropic).unwrap(),
            Some(KeyLocation::Keychain)
        );
        assert_eq!(vault.key(ProviderId::Anthropic).unwrap().expose(), FAKE);
        assert!(!vault.has(ProviderId::Openai).unwrap());

        vault.remove(ProviderId::Anthropic).unwrap();
        assert!(store.accounts().is_empty());
        assert!(!vault.has(ProviderId::Anthropic).unwrap());
    }

    #[test]
    fn without_a_credential_store_it_says_so_and_offers_the_session() {
        let vault = KeyVault::new(Box::new(MemoryStore::unavailable()));
        assert!(!vault.keychain_available());
        let err = vault
            .save(ProviderId::Openai, ApiKey::new(FAKE).unwrap())
            .unwrap_err();
        assert!(err.to_string().contains("for this session only"), "{err}");
        assert!(!err.to_string().contains(FAKE));
        assert!(!vault.has(ProviderId::Openai).unwrap());

        assert_eq!(
            vault.use_for_session(ProviderId::Openai, ApiKey::new(FAKE).unwrap()),
            KeyLocation::Session
        );
        assert_eq!(
            vault.location(ProviderId::Openai).unwrap(),
            Some(KeyLocation::Session)
        );
        assert_eq!(vault.key(ProviderId::Openai).unwrap().expose(), FAKE);
        vault.remove(ProviderId::Openai).unwrap();
        assert!(!vault.has(ProviderId::Openai).unwrap());
    }

    #[test]
    fn the_vault_never_prints_keys() {
        let vault = KeyVault::new(Box::new(MemoryStore::new()));
        vault
            .save(ProviderId::Anthropic, ApiKey::new(FAKE).unwrap())
            .unwrap();
        assert!(!format!("{vault:?}").contains(FAKE));
        assert!(!format!("{:?}", MemoryStore::new()).contains(FAKE));
    }
}
