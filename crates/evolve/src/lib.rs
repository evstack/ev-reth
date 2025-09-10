//! Rollkit-specific types and integration
//!
//! This crate provides Rollkit-specific functionality including:
//! - Custom payload attributes for Rollkit
//! - Rollkit-specific types and traits
//! - Custom consensus implementation

/// Rollkit-specific types and related definitions.
pub mod types;

/// Configuration for Rollkit functionality.
pub mod config;

/// RPC modules for Rollkit functionality.
pub mod rpc;

/// Custom consensus implementation for Rollkit.
pub mod consensus;

#[cfg(test)]
mod tests;

// Re-export public types
pub use config::{RollkitConfig, DEFAULT_MAX_TXPOOL_BYTES, DEFAULT_MAX_TXPOOL_GAS};
pub use consensus::{RollkitConsensus, RollkitConsensusBuilder};
pub use types::{PayloadAttributesError, RollkitPayloadAttributes};
