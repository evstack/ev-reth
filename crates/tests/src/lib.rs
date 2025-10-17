//! Integration tests for ev-reth evolve
//!
//! This crate contains integration tests for the ev-reth evolve implementation,
//! including payload builder tests, engine API tests, and common test utilities.

pub mod common;

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod engine_api_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod payload_builder_tests;
#[cfg(test)]
mod test_evolve_engine_api;

// Re-export common test utilities
pub use common::*;
