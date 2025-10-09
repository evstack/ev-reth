//! EV-specific API wrappers and helpers.

pub mod builder;
pub mod exec;

pub use builder::EvBuilder;
pub use exec::{EvError, EvExecutionResult};
