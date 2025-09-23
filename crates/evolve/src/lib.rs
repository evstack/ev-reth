//! Evolve-specific types and integration
//!
//! This crate provides Evolve-specific functionality including:
//! - Custom payload attributes for Evolve
//! - Evolve-specific types and traits
//! - Custom consensus implementation

/// Evolve-specific types and related definitions.
pub mod types;

/// Configuration for Evolve functionality.
pub mod config;

/// RPC modules for Evolve functionality.
pub mod rpc;

/// Custom consensus implementation for Evolve.
pub mod consensus;

#[cfg(test)]
mod tests;

// Re-export public types
pub use config::{EvolveConfig, DEFAULT_MAX_TXPOOL_BYTES, DEFAULT_MAX_TXPOOL_GAS};
pub use consensus::{EvolveConsensus, EvolveConsensusBuilder};
pub use types::{EvolvePayloadAttributes, PayloadAttributesError};
