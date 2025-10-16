use evolve_ev_reth::PayloadAttributesError;
use thiserror::Error;

/// Custom error type used in payload attributes validation.
#[derive(Debug, Error)]
pub enum EvolveEngineError {
    /// Provided transaction bytes failed to decode.
    #[error("Invalid transaction data: {0}")]
    InvalidTransactionData(String),
    /// Requested payload exceeded allowed gas limit.
    #[error("Gas limit exceeded")]
    GasLimitExceeded,
    /// Underlying evolve payload attribute validation failed.
    #[error("Evolve payload attributes error: {0}")]
    PayloadAttributes(#[from] PayloadAttributesError),
}
