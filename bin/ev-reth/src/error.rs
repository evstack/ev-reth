use evolve_ev_reth::PayloadAttributesError;
use thiserror::Error;

/// Custom error type used in payload attributes validation
#[derive(Debug, Error)]
pub enum EvolveEngineError {
    #[error("Invalid transaction data: {0}")]
    InvalidTransactionData(String),
    #[error("Gas limit exceeded")]
    GasLimitExceeded,
    #[error("Evolve payload attributes error: {0}")]
    PayloadAttributes(#[from] PayloadAttributesError),
}
