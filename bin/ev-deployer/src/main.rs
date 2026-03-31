//! EV Deployer — genesis alloc generator and live deployer for ev-reth contracts.

mod config;
mod contracts;
mod deploy;
mod genesis;
mod init;
mod output;

use alloy_primitives::Address;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// EV Deployer: generate genesis alloc or deploy ev-reth contracts.
#[derive(Parser)]
#[command(
    name = "ev-deployer",
    about = "Generate genesis alloc or deploy ev-reth contracts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a starter config file with all supported contracts.
    Init {
        /// Write config to this file instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Set the chain ID (defaults to 0).
        #[arg(long)]
        chain_id: Option<u64>,

        /// Include `Permit2` with its canonical address.
        #[arg(long)]
        permit2: bool,

        /// Include the deterministic deployer (Nick's factory) with its canonical address.
        #[arg(long)]
        deterministic_deployer: bool,

        /// Include `AdminProxy` with the given owner address.
        #[arg(long)]
        admin_proxy_owner: Option<Address>,
    },
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
    /// Deploy contracts to a live chain via CREATE2.
    Deploy {
        /// Path to the deploy TOML config.
        #[arg(long)]
        config: PathBuf,

        /// RPC URL of the target chain.
        #[arg(long, env = "EV_DEPLOYER_RPC_URL")]
        rpc_url: String,

        /// Hex-encoded private key for signing transactions.
        #[arg(long, env = "EV_DEPLOYER_PRIVATE_KEY")]
        private_key: String,

        /// Path to the state file (created if absent, resumed if present).
        #[arg(long)]
        state: PathBuf,

        /// Write an address manifest to this file.
        #[arg(long)]
        addresses_out: Option<PathBuf>,
    },
    /// Compute the address for a configured contract.
    ComputeAddress {
        /// Path to the deploy TOML config.
        #[arg(long)]
        config: PathBuf,

        /// Contract name (e.g. `admin_proxy`).
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
            cfg.validate_for_genesis()?;

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
        Command::Deploy {
            config: config_path,
            rpc_url,
            private_key,
            state: state_path,
            addresses_out,
        } => {
            let cfg = config::DeployConfig::load(&config_path)?;
            let deployer = deploy::deployer::LiveDeployer::new(&rpc_url, &private_key)?;
            let pipeline_cfg = deploy::pipeline::PipelineConfig {
                config: cfg,
                state_path,
                addresses_out,
            };

            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(deploy::pipeline::run(&pipeline_cfg, &deployer))?;
        }
        Command::Init {
            output,
            chain_id,
            permit2,
            deterministic_deployer,
            admin_proxy_owner,
        } => {
            let params = init::InitParams {
                chain_id: chain_id.unwrap_or(0),
                permit2,
                deterministic_deployer,
                admin_proxy_owner: admin_proxy_owner.map(|a| format!("{a}")),
            };
            let template = init::generate_template(&params);

            if let Some(ref out_path) = output {
                std::fs::write(out_path, &template)?;
                eprintln!("Wrote config to {}", out_path.display());
            } else {
                print!("{template}");
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
                    .and_then(|c| c.address)
                    .ok_or_else(|| eyre::eyre!("admin_proxy not configured or address not set"))?,
                "permit2" => cfg
                    .contracts
                    .permit2
                    .as_ref()
                    .and_then(|c| c.address)
                    .ok_or_else(|| eyre::eyre!("permit2 not configured or address not set"))?,
                "deterministic_deployer" => cfg
                    .contracts
                    .deterministic_deployer
                    .as_ref()
                    .and_then(|c| c.address)
                    .ok_or_else(|| {
                        eyre::eyre!("deterministic_deployer not configured or address not set")
                    })?,
                other => eyre::bail!("unknown contract: {other}"),
            };

            println!("{address}");
        }
    }

    Ok(())
}
