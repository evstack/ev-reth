//! Helpers to build the ev-reth executor with EV-specific hooks applied.

use alloy_evm::eth::{spec::EthExecutorSpec, EthEvmFactory};
use ev_revm::{
    with_ev_handler, BaseFeeRedirect, BaseFeeRedirectSettings, ContractSizeLimitSettings,
    DeployAllowlistSettings, EvEvmFactory, MintPrecompileSettings,
};
use reth_chainspec::ChainSpec;
use reth_ethereum::{
    chainspec::EthereumHardforks,
    evm::EthEvmConfig,
    node::{
        api::FullNodeTypes,
        builder::{components::ExecutorBuilder as RethExecutorBuilder, BuilderContext},
    },
};
use reth_ethereum_forks::Hardforks;
use reth_node_builder::PayloadBuilderConfig;
use tracing::info;

use crate::{config::EvolvePayloadBuilderConfig, EvolveNode};

/// Type alias for the EV-aware EVM config we install into the node.
pub type EvolveEvmConfig = EthEvmConfig<ChainSpec, EvEvmFactory<EthEvmFactory>>;

/// Builds the EV-aware EVM configuration by wrapping the default config with the EV handler.
pub fn build_evm_config<Node>(ctx: &BuilderContext<Node>) -> eyre::Result<EvolveEvmConfig>
where
    Node: FullNodeTypes<Types = EvolveNode>,
    ChainSpec: Hardforks + EthExecutorSpec + EthereumHardforks,
{
    let chain_spec = ctx.chain_spec();
    let base_config = EthEvmConfig::new(chain_spec.clone())
        .with_extra_data(ctx.payload_builder_config().extra_data_bytes());

    let evolve_config = EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref())?;
    evolve_config.validate()?;

    let redirect = evolve_config
        .base_fee_redirect_settings()
        .map(|(sink, activation)| {
            info!(
                target = "ev-reth::executor",
                fee_sink = ?sink,
                activation_height = activation,
                "Base fee redirect enabled"
            );
            BaseFeeRedirectSettings::new(BaseFeeRedirect::new(sink), activation)
        });

    let mint_precompile = evolve_config
        .mint_precompile_settings()
        .map(|(admin, activation)| MintPrecompileSettings::new(admin, activation));

    let contract_size_limit =
        evolve_config
            .contract_size_limit_settings()
            .map(|(limit, activation)| {
                info!(
                    target = "ev-reth::executor",
                    limit_bytes = limit,
                    activation_height = activation,
                    "Custom contract size limit enabled"
                );
                ContractSizeLimitSettings::new(limit, activation)
            });

    let deploy_allowlist =
        evolve_config
            .deploy_allowlist_settings()
            .map(|(allowlist, activation)| {
                info!(
                    target = "ev-reth::executor",
                    allowlist_len = allowlist.len(),
                    activation_height = activation,
                    "Deploy allowlist enabled"
                );
                DeployAllowlistSettings::new(allowlist, activation)
            });

    Ok(with_ev_handler(
        base_config,
        redirect,
        mint_precompile,
        deploy_allowlist,
        contract_size_limit,
    ))
}

/// Thin wrapper so we can plug the EV executor into the node components builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EvolveExecutorBuilder;

impl<Node> RethExecutorBuilder<Node> for EvolveExecutorBuilder
where
    Node: FullNodeTypes<Types = EvolveNode>,
    ChainSpec: Hardforks + EthExecutorSpec + EthereumHardforks,
{
    type EVM = EvolveEvmConfig;

    async fn build_evm(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::EVM> {
        build_evm_config(ctx)
    }
}
