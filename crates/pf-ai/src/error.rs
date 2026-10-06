//! Assistant errors, as plain sentences for the chat panel. None of them ever contains an API
//! key: provider messages are passed through [`sanitize`] first, and key-store errors drop the
//! platform's details (some carry the stored bytes).

use crate::provider::ProviderId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("Add your {0} API key in Settings → AI first.")]
    NoKey(ProviderId),
    #[error(
        "{0} didn't accept your API key. It may be mistyped or revoked: paste it again in Settings → AI."
    )]
    InvalidKey(ProviderId),
    #[error(
        "Your {0} API key isn't allowed to do that. Check its permissions on the {0} website, or pick another model."
    )]
    PermissionDenied(ProviderId),
    #[error("Your {0} account is out of credit or has a billing problem. Check billing on the {0} website.")]
    Billing(ProviderId),
    #[error("{0} is limiting how fast this key can send requests. Wait a minute, then try again.")]
    RateLimited(ProviderId),
    #[error("{0} is busy right now. Try again in a moment.")]
    Overloaded(ProviderId),
    #[error(
        "The model \"{model}\" isn't available to your {provider} key. Pick another model in Settings → AI."
    )]
    ModelNotFound { provider: ProviderId, model: String },
    #[error(
        "The model \"{model}\" can't use tools, and the assistant needs them to read and change your show. Pick another model in Settings → AI."
    )]
    ModelNoTools { provider: ProviderId, model: String },
    #[error("This chat is too long for the model. Start a new chat.")]
    TooLong,
    #[error("Couldn't reach {0}. Check your internet connection, then try again.")]
    Network(ProviderId),
    #[error("{0} took too long to answer. Try again.")]
    Timeout(ProviderId),
    #[error("The connection to {0} dropped before the reply finished. Try again.")]
    Interrupted(ProviderId),
    #[error("{0} sent a reply PixelFlow couldn't read. Try again.")]
    BadResponse(ProviderId),
    #[error("The model declined this request. Try rewording it.")]
    Refused,
    #[error("Stopped.")]
    Cancelled,
    #[error("{provider} reported a problem: {message}")]
    Provider { provider: ProviderId, message: String },
    #[error("{0}")]
    KeyStore(String),
}

/// A provider's own error message made safe to show: anything that looks like a key is hidden
/// (so is `key` itself), it's one line, and it's not too long.
pub fn sanitize(message: &str, key: Option<&str>) -> String {
    let mut text = message.replace(['\n', '\r'], " ");
    if let Some(key) = key.filter(|k| !k.is_empty()) {
        text = text.replace(key, "[your key]");
    }
    let words: Vec<String> = text
        .split(' ')
        .map(|word| {
            let bare =
                word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '*');
            if bare.starts_with("sk-") || bare.starts_with("sk_") {
                word.replace(bare, "[a key]")
            } else {
                word.to_string()
            }
        })
        .collect();
    let mut text = words.join(" ").trim().to_string();
    const LIMIT: usize = 300;
    if text.chars().count() > LIMIT {
        text = text.chars().take(LIMIT).collect::<String>() + "…";
    }
    if text.is_empty() {
        "no details".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_messages_never_carry_keys() {
        let key = "sk-test-not-a-key";
        let shown = sanitize("Incorrect API key provided: sk-test-not-a-key.", Some(key));
        assert!(!shown.contains(key), "{shown}");
        let masked = sanitize(
            "Incorrect API key provided: sk-proj-****abcd. You can find…",
            None,
        );
        assert!(!masked.contains("sk-proj"), "{masked}");
        assert_eq!(sanitize("line one\nline two", None), "line one line two");
        assert_eq!(sanitize("", None), "no details");
        assert!(sanitize(&"x".repeat(1000), None).chars().count() <= 301);
    }

    #[test]
    fn errors_read_as_plain_sentences() {
        assert_eq!(
            AiError::InvalidKey(ProviderId::Anthropic).to_string(),
            "Anthropic didn't accept your API key. It may be mistyped or revoked: paste it again in Settings → AI."
        );
        assert_eq!(
            AiError::RateLimited(ProviderId::Openai).to_string(),
            "OpenAI is limiting how fast this key can send requests. Wait a minute, then try again."
        );
    }
}
