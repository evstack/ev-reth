//! Rollkit-specific types and integration
//!
//! This crate provides Rollkit-specific functionality including:
//! - Custom payload attributes for Rollkit
//! - Rollkit-specific types and traits
//! - Custom consensus implementation

/// Configuration for Rollkit functionality.
pub mod config;

/// RPC modules for Rollkit functionality.
pub mod rpc;

/// Custom consensus implementation for Rollkit.
pub mod consensus;

/// Custom EVM implementation for Rollkit.
pub mod evm;

/// Custom executor implementation for Rollkit.
pub mod executor;

/// Custom precompiles for Rollkit.
mod precompiles;

/// Rollkit-specific types and related definitions.
pub mod types;

#[cfg(test)]
mod tests;

// Re-export public types
pub use config::{RollkitConfig, DEFAULT_MAX_TXPOOL_BYTES};
pub use consensus::{RollkitConsensus, RollkitConsensusBuilder};
pub use types::{PayloadAttributesError, RollkitPayloadAttributes};
