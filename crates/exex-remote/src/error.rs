use thiserror::Error;

/// Errors returned when encoding or decoding remote `ExEx` payloads.
#[derive(Debug, Error)]
pub enum CodecError {
    /// Bincode encoding failed.
    #[error("bincode encode error: {0}")]
    BincodeEncode(#[from] bincode::error::EncodeError),
    /// Bincode decoding failed.
    #[error("bincode decode error: {0}")]
    BincodeDecode(#[from] bincode::error::DecodeError),
    /// The notification envelope has unexpected schema or encoding metadata.
    #[error("invalid notification envelope: {0}")]
    InvalidEnvelope(&'static str),
}

/// Decode-specific alias used by callers that only care about payload parsing.
pub type DecodeError = CodecError;
