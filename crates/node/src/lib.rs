//! ev-reth node implementation
//!
//! This crate provides the core node functionality for ev-reth, including:
//! - Payload builder implementation
//! - Node configuration
//! - RPC interfaces

/// CLI argument handling for evolve-specific options.
pub mod args;
/// Evolve-specific payload attribute wiring.
pub mod attributes;
/// Builder module for payload construction and related utilities.
pub mod builder;
/// Configuration types and validation for the Evolve payload builder.
pub mod config;
/// Shared error types for evolve node wiring.
pub mod error;
/// Executor wiring for EV aware execution.
pub mod executor;
/// Node composition and payload types.
pub mod node;
/// Payload service integration.
pub mod payload_service;
/// Payload validator integration.
pub mod validator;

// Re-export public types for convenience.
pub use args::EvolveArgs;
pub use attributes::{EvolveEnginePayloadAttributes, EvolveEnginePayloadBuilderAttributes};
pub use builder::{create_payload_builder_service, EvolvePayloadBuilder};
pub use config::{ConfigError, EvolvePayloadBuilderConfig};
pub use error::EvolveEngineError;
pub use executor::{build_evm_config, EvolveEvmConfig, EvolveExecutorBuilder};
pub use node::{log_startup, EvolveEngineTypes, EvolveNode, EvolveNodeAddOns};
pub use payload_service::{EvolveEnginePayloadBuilder, EvolvePayloadBuilderBuilder};
pub use validator::{EvolveEngineValidator, EvolveEngineValidatorBuilder};
