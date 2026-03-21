//! Minimal in-process `ExEx` example for ev-reth.
//!
//! This mirrors the Reth `ExEx` pattern but keeps the handler local and small.

use clap::Parser;
use ev_node::{EvolveArgs, EvolveChainSpecParser, EvolveNode};
use ev_primitives::EvPrimitives;
use futures::TryStreamExt;
use reth_ethereum::node::api::{FullNodeComponents, NodeTypes};
use reth_ethereum_cli::Cli;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use tracing::info;

async fn block_logger<Node>(mut ctx: ExExContext<Node>) -> eyre::Result<()>
where
    Node: FullNodeComponents<Types: NodeTypes<Primitives = EvPrimitives>>,
{
    while let Some(notification) = ctx.notifications.try_next().await? {
        match notification {
            ExExNotification::ChainCommitted { new } => {
                info!(
                    committed_range = ?new.range(),
                    committed_tip = ?new.tip().num_hash(),
                    "received committed chain"
                );
                ctx.events
                    .send(ExExEvent::FinishedHeight(new.tip().num_hash()))?;
            }
            ExExNotification::ChainReorged { old, new } => {
                info!(
                    from_range = ?old.range(),
                    to_range = ?new.range(),
                    "received reorg"
                );
                ctx.events
                    .send(ExExEvent::FinishedHeight(new.tip().num_hash()))?;
            }
            ExExNotification::ChainReverted { old } => {
                info!(reverted_range = ?old.range(), "received revert");
            }
        }
    }

    Ok(())
}

fn main() -> eyre::Result<()> {
    Cli::<EvolveChainSpecParser, EvolveArgs>::parse().run(|builder, _evolve_args| async move {
        let handle = builder
            .node(EvolveNode::new())
            .install_exex("block-logger", |ctx| async move { Ok(block_logger(ctx)) })
            .launch()
            .await?;

        handle.node_exit_future.await
    })
}
