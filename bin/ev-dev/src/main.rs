//! ev-dev: one-command local dev chain for ev-reth.
//!
//! Spins up a fully functional Evolve chain with funded accounts,
//! similar to Hardhat Node or Anvil.

#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use alloy_signer_local::{coins_bip39::English, MnemonicBuilder};
use clap::Parser;
use evolve_ev_reth::{
    config::EvolveConfig,
    rpc::txpool::{EvolveTxpoolApiImpl, EvolveTxpoolApiServer},
};
use reth_ethereum_cli::Cli;
use std::io::Write;
use tracing::info;

use ev_node::{EvolveArgs, EvolveChainSpecParser, EvolveNode};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

const DEVNET_GENESIS: &str = include_str!("../assets/devnet-genesis.json");
const HARDHAT_MNEMONIC: &str = "test test test test test test test test test test test junk";

/// Local dev chain for ev-reth with pre-funded accounts.
#[derive(Parser, Debug)]
#[command(name = "ev-dev", about = "One-command local Evolve dev chain")]
struct EvDevArgs {
    /// Host to bind HTTP/WS RPC server
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port for HTTP/WS RPC server
    #[arg(long, default_value_t = 8545)]
    port: u16,

    /// Block time in seconds (0 = mine on tx)
    #[arg(long, default_value_t = 1)]
    block_time: u64,

    /// Suppress the startup banner
    #[arg(long, default_value_t = false)]
    silent: bool,

    /// Number of accounts to display (max 20)
    #[arg(long, default_value_t = 10)]
    accounts: usize,
}

fn derive_keys(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|i| {
            let signer = MnemonicBuilder::<English>::default()
                .phrase(HARDHAT_MNEMONIC)
                .index(i as u32)
                .expect("valid derivation index")
                .build()
                .expect("valid key derivation");
            let address = signer.address();
            let key_bytes = signer.credential().to_bytes();
            (
                format!("{address}"),
                format!("0x{}", alloy_primitives::hex::encode(key_bytes)),
            )
        })
        .collect()
}

fn chain_id_from_genesis() -> u64 {
    let genesis: serde_json::Value =
        serde_json::from_str(DEVNET_GENESIS).expect("valid genesis JSON");
    genesis["config"]["chainId"]
        .as_u64()
        .expect("genesis must have config.chainId")
}

fn print_banner(args: &EvDevArgs) {
    let count = args.accounts.min(20);
    let accounts = derive_keys(count);

    println!();
    println!(r"                       _            ");
    println!(r"                      | |           ");
    println!(r"   _____   _____   __| | _____   __");
    println!(r"  / _ \ \ / /___/ / _` |/ _ \ \ / /");
    println!(r" |  __/\ V /    | (_| |  __/\ V / ");
    println!(r"  \___| \_/      \__,_|\___| \_/  ");
    println!();
    println!("  Evolve Local Development Chain");
    println!("  ==============================");
    println!();
    println!("Chain ID:      {}", chain_id_from_genesis());
    println!("RPC URL:       http://{}:{}", args.host, args.port);
    println!(
        "Block time:    {}",
        if args.block_time == 0 {
            "auto (mine on tx)".to_string()
        } else {
            format!("{}s", args.block_time)
        }
    );
    println!();
    println!("Available Accounts");
    println!("==================");
    for (i, (addr, _)) in accounts.iter().enumerate() {
        println!("({i}) {addr} (1000 ETH)");
    }
    println!();
    println!("Private Keys");
    println!("==================");
    for (i, (_, key)) in accounts.iter().enumerate() {
        println!("({i}) {key}");
    }
    println!();
    println!("Mnemonic: {HARDHAT_MNEMONIC}");
    println!("Derivation path: m/44'/60'/0'/0/{{index}}");
    println!();
    println!("WARNING: These accounts and keys are publicly known.");
    println!("Any funds sent to them on mainnet WILL BE LOST.");
    println!();
}

fn main() {
    reth_cli_util::sigsegv_handler::install();

    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    let dev_args = EvDevArgs::parse();

    if !dev_args.silent {
        print_banner(&dev_args);
    }

    // Write genesis to a temp file that lives for the process duration
    let mut genesis_file =
        tempfile::NamedTempFile::new().expect("failed to create temp genesis file");
    genesis_file
        .write_all(DEVNET_GENESIS.as_bytes())
        .expect("failed to write genesis");
    let genesis_path = genesis_file
        .path()
        .to_str()
        .expect("valid path")
        .to_string();

    // Use a temp data directory so each run starts with clean state
    let datadir = tempfile::TempDir::new().expect("failed to create temp data dir");
    let datadir_path = datadir
        .path()
        .to_str()
        .expect("valid path")
        .to_string();

    let mut args = vec![
        "ev-dev".to_string(),
        "node".to_string(),
        "--dev".to_string(),
        "--chain".to_string(),
        genesis_path,
        "--datadir".to_string(),
        datadir_path,
        "--http".to_string(),
        "--http.addr".to_string(),
        dev_args.host.clone(),
        "--http.port".to_string(),
        dev_args.port.to_string(),
        "--http.api".to_string(),
        "eth,net,web3,txpool,debug,trace".to_string(),
        "--http.corsdomain".to_string(),
        "*".to_string(),
        "--ws".to_string(),
        "--ws.addr".to_string(),
        dev_args.host.clone(),
        "--ws.port".to_string(),
        dev_args.port.to_string(),
        "--ws.api".to_string(),
        "eth,net,web3,txpool,debug,trace".to_string(),
        "--disable-discovery".to_string(),
        "--no-persist-peers".to_string(),
        "--port".to_string(),
        "0".to_string(),
    ];

    if dev_args.block_time > 0 {
        args.push("--dev.block-time".to_string());
        args.push(format!("{}s", dev_args.block_time));
    }

    if let Err(err) = Cli::<EvolveChainSpecParser, EvolveArgs>::try_parse_from(args)
        .expect("valid CLI args")
        .run(|builder, _evolve_args| async move {
            info!("=== EV-DEV: Starting local development chain ===");
            let handle = builder
                .node(EvolveNode::new())
                .extend_rpc_modules(move |ctx| {
                    let evolve_cfg = EvolveConfig::default();
                    let evolve_txpool =
                        EvolveTxpoolApiImpl::new(ctx.pool().clone(), evolve_cfg.max_txpool_bytes);
                    ctx.modules.merge_configured(evolve_txpool.into_rpc())?;
                    Ok(())
                })
                .launch_with_debug_capabilities()
                .await?;

            info!("=== EV-DEV: Local chain running - RPC ready ===");
            handle.node_exit_future.await
        })
    {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
