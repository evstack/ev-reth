use crate::evm::RollkitEvmFactory;
use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        api::{FullNodeTypes, NodeTypes},
        builder::{components::ExecutorBuilder, BuilderContext},
        EthEvmConfig,
    },
    EthPrimitives,
};

/// Rollkit executor builder that utilizes the Rollkit EVM factory.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct RollkitExecutorBuilder;

impl<Node> ExecutorBuilder<Node> for RollkitExecutorBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = ChainSpec, Primitives = EthPrimitives>>,
{
    type EVM = EthEvmConfig<ChainSpec, RollkitEvmFactory>;

    async fn build_evm(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::EVM> {
        let evm_config =
            EthEvmConfig::new_with_evm_factory(ctx.chain_spec(), RollkitEvmFactory::default());
        Ok(evm_config)
    }
}
