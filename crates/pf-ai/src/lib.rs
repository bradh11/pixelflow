//! PixelFlow's bring-your-own-key AI assistant.
//!
//! - [`LlmProvider`]: Anthropic ([`anthropic::Anthropic`]) and OpenAI ([`openai::OpenAi`]),
//!   streaming replies with tool calls over HTTPS from Rust. The window never sees a key and
//!   never talks to a provider.
//! - [`KeyVault`]: keys live in the OS credential store (one entry per provider under
//!   `com.bradh11.pixelflow`), or in memory for one session when there is none. Keys are
//!   [`ApiKey`]s: never printed, never serialized, zeroed when dropped.
//! - [`Toolbox`]: one tool per engine edit, generated from the edit types' JSON Schemas, plus
//!   read-only queries and the draft tools. There are no tools for files, output, or devices.
//! - [`ChatSession`]: the tool loop. The model edits a private [`Draft`]; its [`Proposal`]
//!   (summary, [`Diff`], preview) changes the show only through [`apply_proposal`], from the
//!   user's Apply, as one undo step.

pub mod agent;
pub mod align;
pub mod anthropic;
pub mod arrange;
pub mod cues;
pub mod diff;
pub mod draft;
pub mod error;
pub mod http;
pub mod keys;
pub mod lyrics;
pub mod openai;
pub mod provider;
pub mod review;
pub mod run;
pub mod secret;
pub mod song;
pub mod sse;
pub mod summary;
pub mod tools;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod testing;

pub use agent::{ChatEvent, ChatSession, TurnReply};
pub use diff::{Action, Change, Diff, Section};
pub use draft::{Applied, Draft, OpenDoc, Proposal, ProposalView, UiContext, Workspace, apply_proposal};
pub use error::AiError;
pub use keys::{CredentialStore, KeyLocation, KeyVault, MemoryStore, OsKeychain, keychain_name};
pub use provider::{Cancel, LlmProvider, ModelInfo, ProviderId};
pub use secret::ApiKey;
pub use tools::Toolbox;

use std::sync::Arc;

/// Both providers over one transport.
pub fn providers(transport: Arc<dyn http::Transport>) -> Providers {
    Providers {
        anthropic: Arc::new(anthropic::Anthropic::new(transport.clone())),
        openai: Arc::new(openai::OpenAi::new(transport)),
    }
}

/// The providers the app can use.
#[derive(Clone)]
pub struct Providers {
    pub anthropic: Arc<dyn LlmProvider>,
    pub openai: Arc<dyn LlmProvider>,
}

impl Providers {
    pub fn get(&self, id: ProviderId) -> Arc<dyn LlmProvider> {
        match id {
            ProviderId::Anthropic => self.anthropic.clone(),
            ProviderId::Openai => self.openai.clone(),
        }
    }
}
