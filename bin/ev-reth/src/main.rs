//! Evolve node binary with standard reth CLI support and evolve payload builder integration.
//!
//! This node supports all standard reth CLI flags and functionality, with a customized
//! payload builder that accepts transactions via engine API payload attributes.

#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use clap::Parser;
use reth_ethereum_cli::Cli;
use reth_tracing_otlp::{OtlpConfig, OtlpProtocol};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use url::Url;

use ev_node::{head::HeadPublisher, log_startup, EvolveArgs, EvolveChainSpecParser, EvolveNode};

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

const EV_TRACE_LEVEL_ENV: &str = "EV_TRACE_LEVEL";

/// Initialize tracing with optional OTLP support.
///
/// When OTLP is enabled, per-layer filtering is applied so that stdout logs
/// are controlled by `RUST_LOG` while the OTLP span exporter is controlled
/// by `EV_TRACE_LEVEL` (falling back to `RUST_LOG`, then `"info"`).
fn init_tracing() {
    if let Some(config) = otlp_config_from_env() {
        if let Ok(otlp_layer) = reth_tracing_otlp::span_layer(config) {
            let log_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

            let trace_filter = std::env::var(EV_TRACE_LEVEL_ENV)
                .ok()
                .and_then(|val| EnvFilter::try_new(val).ok())
                .unwrap_or_else(|| {
                    EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
                });

            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_target(false)
                        .with_filter(log_filter),
                )
                .with(otlp_layer.with_filter(trace_filter))
                .init();

            info!("OTLP tracing initialized for service: ev-reth");
            return;
        }
    }

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();
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
        Cli::<EvolveChainSpecParser, EvolveArgs>::parse().run(|builder, evolve_args| async move {
            log_startup();
            let head_publisher = HeadPublisher::new(
                builder.config().chain.chain().id(),
                builder.config().chain.genesis_hash(),
            );

            let handle = builder
                .node(EvolveNode::new().with_head_publisher(head_publisher.clone()))
                .launch()
                .await?;

            ev_node::head::spawn_publisher(
                handle.node.task_executor.clone(),
                handle.node.provider.clone(),
                handle
                    .node
                    .add_ons_handle
                    .consensus_engine_events()
                    .new_listener(),
                head_publisher,
            );

            if let Some(peer_url) = evolve_args.subscribe_peer {
                ev_node::head::spawn_subscriber(
                    handle.node.task_executor.clone(),
                    handle.node.add_ons_handle.beacon_engine_handle.clone(),
                    peer_url,
                    handle.node.config.chain.chain().id(),
                    handle.node.config.chain.genesis_hash(),
                );
            }

            info!("=== EV-RETH: Node launched successfully with ev-reth payload builder ===");
            handle.node_exit_future.await
        })
    {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
