//! PixelFlow's bring-your-own-key AI assistant.
//!
//! - [`LlmProvider`]: Anthropic ([`anthropic::Anthropic`]) and OpenAI ([`openai::OpenAi`]),
//!   streaming replies with tool calls over HTTPS from Rust. The window never sees a key and
//!   never talks to a provider.
//! - [`KeyVault`]: keys live in the OS credential store (one entry per provider under
//!   `com.bradh11.pixelflow`), or in memory for one session when there is none. Keys are
//!   [`ApiKey`]s: never printed, never serialized, zeroed when dropped.

pub mod anthropic;
pub mod error;
pub mod http;
pub mod keys;
pub mod openai;
pub mod provider;
pub mod secret;
pub mod sse;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod testing;

pub use error::AiError;
pub use keys::{CredentialStore, KeyLocation, KeyVault, MemoryStore, OsKeychain, keychain_name};
pub use provider::{Cancel, LlmProvider, ModelInfo, ProviderId};
pub use secret::ApiKey;
