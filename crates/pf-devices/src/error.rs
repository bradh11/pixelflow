//! Device errors, written for people.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeviceError {
    #[error("Could not reach {address}: {reason}")]
    Unreachable { address: String, reason: String },
    #[error("{address} answered HTTP {status} for {path}.")]
    Http {
        address: String,
        path: String,
        status: u16,
    },
    #[error("{address} sent a response PixelFlow doesn't understand ({path}): {reason}")]
    BadResponse {
        address: String,
        path: String,
        reason: String,
    },
    /// A ready-made plain-language message.
    #[error("{0}")]
    Message(String),
    #[error("{0} doesn't look like an FPP, Falcon, or WLED controller.")]
    Unrecognized(String),
}

impl DeviceError {
    pub(crate) fn bad_plain(message: impl Into<String>) -> Self {
        DeviceError::Message(message.into())
    }

    pub(crate) fn bad(address: &str, path: &str, reason: impl Into<String>) -> Self {
        DeviceError::BadResponse {
            address: address.to_string(),
            path: path.to_string(),
            reason: reason.into(),
        }
    }
}
