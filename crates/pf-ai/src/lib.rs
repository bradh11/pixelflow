//! PixelFlow's bring-your-own-key AI assistant.
//!
//! - [`KeyVault`]: keys live in the OS credential store (one entry per provider under
//!   `com.bradh11.pixelflow`), or in memory for one session when there is none. Keys are
//!   [`ApiKey`]s: never printed, never serialized, zeroed when dropped.

pub mod error;
pub mod keys;
pub mod provider;
pub mod secret;

pub use error::AiError;
pub use keys::{CredentialStore, KeyLocation, KeyVault, MemoryStore, OsKeychain, keychain_name};
pub use provider::{Cancel, LlmProvider, ModelInfo, ProviderId};
pub use secret::ApiKey;
