use thiserror::Error;

/// Errors returned when encoding or decoding remote `ExEx` payloads.
#[derive(Debug, Error)]
pub enum CodecError {
    /// Bincode serialization or deserialization failed.
    #[error("bincode codec error: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    /// The notification envelope has unexpected schema or encoding metadata.
    #[error("invalid notification envelope: {0}")]
    InvalidEnvelope(&'static str),
}

/// Decode-specific alias used by callers that only care about payload parsing.
pub type DecodeError = CodecError;
