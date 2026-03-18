//! EV Deployer — genesis alloc generator for ev-reth contracts.

mod config;
mod contracts;
mod genesis;
mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// EV Deployer: generate genesis alloc entries for ev-reth contracts.
#[derive(Parser)]
#[command(
    name = "ev-deployer",
    about = "Generate genesis alloc for ev-reth contracts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate genesis alloc JSON from a deploy config.
    Genesis {
        /// Path to the deploy TOML config.
        #[arg(long)]
        config: PathBuf,

        /// Write alloc JSON to this file instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Merge alloc entries into an existing genesis JSON file.
        #[arg(long)]
        merge_into: Option<PathBuf>,

        /// Allow overwriting existing addresses when merging.
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Write an address manifest to this file.
        #[arg(long)]
        addresses_out: Option<PathBuf>,
    },
    /// Compute the address for a configured contract.
    ComputeAddress {
        /// Path to the deploy TOML config.
        #[arg(long)]
        config: PathBuf,

        /// Contract name (`admin_proxy` or `fee_vault`).
        #[arg(long)]
        contract: String,
    },
}

fn main() -> eyre::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Genesis {
            config: config_path,
            output,
            merge_into,
            force,
            addresses_out,
        } => {
            let cfg = config::DeployConfig::load(&config_path)?;

            let result = if let Some(ref genesis_path) = merge_into {
                genesis::merge_into(&cfg, genesis_path, force)?
            } else {
                genesis::build_alloc(&cfg)
            };

            let json = serde_json::to_string_pretty(&result)?;

            if let Some(ref out_path) = output {
                std::fs::write(out_path, &json)?;
                eprintln!("Wrote alloc to {}", out_path.display());
            } else {
                println!("{json}");
            }

            if let Some(ref addr_path) = addresses_out {
                let manifest = output::build_manifest(&cfg);
                let manifest_json = serde_json::to_string_pretty(&manifest)?;
                std::fs::write(addr_path, &manifest_json)?;
                eprintln!("Wrote address manifest to {}", addr_path.display());
            }
        }
        Command::ComputeAddress {
            config: config_path,
            contract,
        } => {
            let cfg = config::DeployConfig::load(&config_path)?;

            let address = match contract.as_str() {
                "admin_proxy" => cfg
                    .contracts
                    .admin_proxy
                    .as_ref()
                    .map(|c| c.address)
                    .ok_or_else(|| eyre::eyre!("admin_proxy not configured"))?,
                "fee_vault" => cfg
                    .contracts
                    .fee_vault
                    .as_ref()
                    .map(|c| c.address)
                    .ok_or_else(|| eyre::eyre!("fee_vault not configured"))?,
                "merkle_tree_hook" => cfg
                    .contracts
                    .merkle_tree_hook
                    .as_ref()
                    .map(|c| c.address)
                    .ok_or_else(|| eyre::eyre!("merkle_tree_hook not configured"))?,
                other => eyre::bail!("unknown contract: {other}"),
            };

            println!("{address}");
        }
    }

    Ok(())
}
