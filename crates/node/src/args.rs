use clap::Args;

/// Evolve CLI arguments.
#[derive(Debug, Clone, Default, Args)]
pub struct EvolveArgs {
    /// Block height at which the Eden WTIA storage hardfork activates.
    #[arg(long)]
    pub eden_hardfork_height: Option<u64>,
}
