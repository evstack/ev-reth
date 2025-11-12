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
use reth_tracing_otlp::{span_layer, OtlpProtocol};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use ev_node::{log_startup, EvolveArgs, EvolveNode};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

/// Initialize reth OTLP tracing
fn init_otlp_tracing() -> eyre::Result<()> {
    const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:4318/v1/traces";

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned());
    let mut endpoint = Url::parse(&endpoint).or_else(|_| Url::parse(DEFAULT_ENDPOINT))?;

    let protocol = std::env::var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL"))
        .unwrap_or_else(|_| "http".to_string());
    let protocol = match protocol.to_lowercase().as_str() {
        "grpc" => OtlpProtocol::Grpc,
        _ => OtlpProtocol::Http,
    };

    protocol.validate_endpoint(&mut endpoint)?;

    let span_layer = span_layer("ev-reth", &endpoint, protocol)?;
    // Set up tracing subscriber with reth OTLP layer
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(span_layer)
        .init();

    info!("Reth OTLP tracing initialized for service: ev-reth");
    Ok(())
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
        if let Err(e) = init_otlp_tracing() {
            eprintln!("Failed to initialize OTLP tracing: {:?}", e);
            eprintln!("Continuing without OTLP tracing...");
        }
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
