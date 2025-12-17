//! Evolve node binary with standard reth CLI support and evolve payload builder integration.
//!
//! This node supports all standard reth CLI flags and functionality, with a customized
//! payload builder that accepts transactions via engine API payload attributes.

#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use clap::Parser;
use evolve_ev_reth::{
    config::EvolveConfig,
    rpc::txpool::{EvolveTxpoolApiImpl, EvolveTxpoolApiServer},
};
use reth_ethereum_cli::{chainspec::EthereumChainSpecParser, Cli};
use reth_tracing_otlp::layer as otlp_layer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use ev_node::{log_startup, EvolveArgs, EvolveNode};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

/// Initialize reth OTLP tracing
fn init_otlp_tracing() {
    // Set up tracing subscriber with reth OTLP layer
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(otlp_layer("ev-reth"))
        .init();

    info!("Reth OTLP tracing initialized for service: ev-reth");
}

fn main() {
    info!("=== EV-RETH NODE STARTING ===");

    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Initialize OTLP tracing
    if std::env::var("OTEL_SDK_DISABLED").as_deref() == Ok("false") {
        init_otlp_tracing();
    }

    if let Err(err) = Cli::<EthereumChainSpecParser, EvolveArgs>::parse().run(
        |builder, _evolve_args| async move {
            log_startup();
            let handle = builder
                .node(EvolveNode::new())
                .extend_rpc_modules(move |ctx| {
                    // Build custom txpool RPC with config + optional CLI/env override
                    let evolve_cfg = EvolveConfig::default();
                    let evolve_txpool =
                        EvolveTxpoolApiImpl::new(ctx.pool().clone(), evolve_cfg.max_txpool_bytes);

                    // Merge into all enabled transports (HTTP / WS)
                    ctx.modules.merge_configured(evolve_txpool.into_rpc())?;
                    Ok(())
                })
                .launch()
                .await?;

            info!("=== EV-RETH: Node launched successfully with ev-reth payload builder ===");
            handle.node_exit_future.await
        },
    ) {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
