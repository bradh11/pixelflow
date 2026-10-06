//! API keys: held only in Rust, never printed, zeroed when dropped.

use serde::{Deserialize, Deserializer};
use std::fmt;
use zeroize::Zeroizing;

/// Longest key accepted (real keys are a few hundred characters at most).
const MAX_KEY_LEN: usize = 1024;

/// A provider API key. Its `Debug` and `Display` never show it, it can't be serialized (so it
/// can't be sent back to the window or written into a file by accident), and its memory is
/// zeroed when it's dropped.
#[derive(Clone)]
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    /// Checks a key as typed (surrounding spaces are dropped): it must be non-empty, one line of
    /// printable ASCII, and of a sane length. The reason never repeats the key.
    pub fn new(text: impl Into<String>) -> Result<Self, KeyFormatError> {
        let text = Zeroizing::new(text.into());
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(KeyFormatError::Empty);
        }
        if trimmed.len() > MAX_KEY_LEN {
            return Err(KeyFormatError::TooLong);
        }
        if !trimmed.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(KeyFormatError::BadCharacters);
        }
        Ok(Self(Zeroizing::new(trimmed.to_string())))
    }

    /// The key itself, for the one place that needs it: the request header sent to the provider.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Keys arrive from the window as text (write-only: there is no way back).
impl<'de> Deserialize<'de> for ApiKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = Zeroizing::new(String::deserialize(deserializer)?);
        ApiKey::new(text.as_str()).map_err(serde::de::Error::custom)
    }
}

/// Why a typed key can't be used. Never includes the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyFormatError {
    #[error("Paste your API key first.")]
    Empty,
    #[error("That's too long to be an API key. Paste just the key.")]
    TooLong,
    #[error("An API key is one line of letters, digits, and dashes. Paste just the key.")]
    BadCharacters,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-test-not-a-key";

    #[test]
    fn keys_never_show_in_debug_or_display() {
        let key = ApiKey::new(FAKE).unwrap();
        assert!(!format!("{key:?}").contains(FAKE));
        assert!(!format!("{key}").contains(FAKE));
        assert!(!format!("{:?}", Some(key.clone())).contains(FAKE));
        assert_eq!(key.expose(), FAKE);
    }

    #[test]
    fn typed_keys_are_trimmed_and_checked() {
        assert_eq!(ApiKey::new(format!("  {FAKE}\n")).unwrap().expose(), FAKE);
        assert_eq!(ApiKey::new("   ").unwrap_err(), KeyFormatError::Empty);
        assert_eq!(ApiKey::new("sk test").unwrap_err(), KeyFormatError::BadCharacters);
        assert_eq!(
            ApiKey::new("sk-\u{e9}").unwrap_err(),
            KeyFormatError::BadCharacters
        );
        assert_eq!(
            ApiKey::new("x".repeat(2000)).unwrap_err(),
            KeyFormatError::TooLong
        );
    }

    #[test]
    fn keys_come_in_from_json_but_errors_never_echo_them() {
        let key: ApiKey = serde_json::from_str(&format!("\"{FAKE}\"")).unwrap();
        assert_eq!(key.expose(), FAKE);
        let err = serde_json::from_str::<ApiKey>("\"sk bad key\"")
            .unwrap_err()
            .to_string();
        assert!(!err.contains("sk bad key"), "{err}");
    }
}
