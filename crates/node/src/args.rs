use clap::Parser;
use serde::{Deserialize, Serialize};

/// Evolve-specific command line arguments.
#[derive(Debug, Clone, Parser, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvolveArgs {
    /// Enable Evolve mode for the node (enabled by default).
    #[arg(
        long = "ev-reth.enable",
        default_value = "true",
        help = "Enable Evolve integration for transaction processing via Engine API"
    )]
    pub enable_evolve: bool,
}
