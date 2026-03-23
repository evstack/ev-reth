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
/// Chainspec parser with ev-reth overrides.
pub mod chainspec;
/// Configuration types and validation for the Evolve payload builder.
pub mod config;
/// Shared error types for evolve node wiring.
pub mod error;
/// EV-specific EVM executor building blocks.
pub mod evm_executor;
/// Executor wiring for EV aware execution.
pub mod executor;
/// Execution extension support for remote consumers.
#[cfg(feature = "remote-exex")]
pub mod exex;
/// Node composition and payload types.
pub mod node;
/// Payload service integration.
pub mod payload_service;
/// Payload types for `EvPrimitives`.
pub mod payload_types;
/// RPC wiring for EvTxEnvelope support.
pub mod rpc;
/// Drop guard for recording `duration_ms` on tracing spans.
pub(crate) mod tracing_ext;
/// Transaction pool wiring and validation.
pub mod txpool;
/// Payload validator integration.
pub mod validator;

#[cfg(test)]
mod test_utils;

// Re-export public types for convenience.
pub use args::EvolveArgs;
pub use attributes::{EvolveEnginePayloadAttributes, EvolveEnginePayloadBuilderAttributes};
pub use builder::{create_payload_builder_service, EvolvePayloadBuilder};
pub use chainspec::EvolveChainSpecParser;
pub use config::{ConfigError, EvolvePayloadBuilderConfig};
pub use error::EvolveEngineError;
pub use executor::{build_evm_config, EvolveEvmConfig, EvolveExecutorBuilder};
#[cfg(feature = "remote-exex")]
pub use exex::{remote_exex_task, spawn_remote_exex_grpc_server, RemoteExExConfig, REMOTE_EXEX_ID};
pub use node::{log_startup, EvolveEngineTypes, EvolveNode, EvolveNodeAddOns};
pub use payload_service::{EvolveEnginePayloadBuilder, EvolvePayloadBuilderBuilder};
pub use payload_types::EvBuiltPayload;
pub use validator::{EvolveEngineValidator, EvolveEngineValidatorBuilder};
