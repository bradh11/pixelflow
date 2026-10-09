//! Whether lyrics can be found: only once the assistant is set up (a provider chosen, and its key
//! in the credential store or kept for this session). The words can also be heard from the song
//! itself only with an OpenAI key (Anthropic has no speech recognition). The key is only checked
//! for, never read out.

use crate::keys::KeyVault;
use crate::provider::ProviderId;
use serde::Serialize;

/// What the lyrics button can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsGate {
    pub ready: bool,
    /// Why not, in one line, when not ready.
    pub reason: Option<String>,
    /// Whether the song's audio can be sent to OpenAI to hear the words.
    pub recognizer: bool,
}

impl LyricsGate {
    fn off(reason: String) -> Self {
        Self {
            ready: false,
            reason: Some(reason),
            recognizer: false,
        }
    }
}

/// The gate for the chosen `provider` (`None`: none chosen yet).
pub fn gate(vault: &KeyVault, provider: Option<ProviderId>) -> LyricsGate {
    let Some(provider) = provider else {
        return LyricsGate::off("Finding lyrics needs the assistant: set it up in Settings → AI.".into());
    };
    match vault.has(provider) {
        Ok(true) => LyricsGate {
            ready: true,
            reason: None,
            recognizer: provider == ProviderId::Openai,
        },
        Ok(false) => LyricsGate::off(format!(
            "Finding lyrics needs the assistant: add your {provider} key in Settings → AI."
        )),
        Err(error) => LyricsGate::off(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{CredentialStore, MemoryStore, StoreError};
    use crate::secret::ApiKey;
    use crate::testing::fake_key;

    #[test]
    fn no_assistant_no_lyrics() {
        let vault = KeyVault::new(Box::new(MemoryStore::new()));
        let gate = super::gate(&vault, None);
        assert!(!gate.ready && !gate.recognizer);
        assert!(gate.reason.unwrap().contains("Settings → AI"));
        let gate = super::gate(&vault, Some(ProviderId::Anthropic));
        assert!(!gate.ready);
        assert!(gate.reason.unwrap().contains("add your Anthropic key"));
    }

    #[test]
    fn an_anthropic_key_finds_published_lyrics_only() {
        let vault = KeyVault::new(Box::new(MemoryStore::new()));
        vault.save(ProviderId::Anthropic, fake_key()).unwrap();
        let gate = super::gate(&vault, Some(ProviderId::Anthropic));
        assert_eq!(
            gate,
            LyricsGate {
                ready: true,
                reason: None,
                recognizer: false
            }
        );
        // An OpenAI key saved but Anthropic chosen: still no recognizer.
        vault.save(ProviderId::Openai, fake_key()).unwrap();
        assert!(!super::gate(&vault, Some(ProviderId::Anthropic)).recognizer);
    }

    #[test]
    fn an_openai_key_can_also_hear_the_words() {
        let vault = KeyVault::new(Box::new(MemoryStore::unavailable()));
        assert!(!super::gate(&vault, Some(ProviderId::Openai)).ready);
        vault.use_for_session(ProviderId::Openai, fake_key());
        let gate = super::gate(&vault, Some(ProviderId::Openai));
        assert!(gate.ready && gate.recognizer);
    }

    /// A store that's locked.
    struct Locked;
    impl CredentialStore for Locked {
        fn get(&self, _: &str) -> Result<Option<ApiKey>, StoreError> {
            Err(StoreError::Denied)
        }
        fn set(&self, _: &str, _: &ApiKey) -> Result<(), StoreError> {
            Err(StoreError::Denied)
        }
        fn delete(&self, _: &str) -> Result<(), StoreError> {
            Err(StoreError::Denied)
        }
        fn available(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_locked_keychain_says_so() {
        let gate = super::gate(&KeyVault::new(Box::new(Locked)), Some(ProviderId::Openai));
        assert!(!gate.ready);
        assert!(gate.reason.unwrap().contains("locked"));
    }
}
