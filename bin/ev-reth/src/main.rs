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
use reth_ethereum_cli::Cli;
use reth_tracing_otlp::{OtlpConfig, OtlpProtocol};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use url::Url;

use ev_node::{log_startup, EvolveArgs, EvolveChainSpecParser, EvolveNode};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

/// Builds OTLP config from environment variables.
/// Returns None if OTLP is disabled or endpoint is not configured.
fn otlp_config_from_env() -> Option<OtlpConfig> {
    // disabled if OTEL_SDK_DISABLED is set to "true" (case-insensitive) per OpenTelemetry spec
    if std::env::var("OTEL_SDK_DISABLED").is_ok_and(|v| v.eq_ignore_ascii_case("true")) {
        return None;
    }

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;
    let endpoint_url = Url::parse(&endpoint).ok()?;

    let protocol = match std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .unwrap_or_else(|_| "http".to_string())
        .as_str()
    {
        "grpc" => OtlpProtocol::Grpc,
        _ => OtlpProtocol::Http,
    };

    OtlpConfig::new("ev-reth", endpoint_url, protocol, None).ok()
}

/// Initialize tracing with optional OTLP support.
fn init_tracing() {
    let registry = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false));

    if let Some(config) = otlp_config_from_env() {
        if let Ok(otlp_layer) = reth_tracing_otlp::span_layer(config) {
            registry.with(otlp_layer).init();
            info!("OTLP tracing initialized for service: ev-reth");
            return;
        }
    }

    registry.init();
}

fn main() {
    info!("=== EV-RETH NODE STARTING ===");

    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // initialize tracing (with optional OTLP support based on env vars)
    init_tracing();

    if let Err(err) =
        Cli::<EvolveChainSpecParser, EvolveArgs>::parse().run(|builder, _evolve_args| async move {
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
        })
    {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
