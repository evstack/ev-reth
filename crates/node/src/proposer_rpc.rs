#![allow(
    clippy::double_must_use,
    reason = "jsonrpsee's async trait expansion adds must_use to an already must-use future"
)]

//! RPC accessors for Evolve proposer control state.

use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{B256, U256};
use async_trait::async_trait;
use jsonrpsee_core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use jsonrpsee_types::ErrorObjectOwned;
use reth_storage_api::StateProviderFactory;

const NEXT_PROPOSER_SLOT: B256 = B256::ZERO;
const INTERNAL_ERROR: i32 = -32603;

/// Evolve proposer-control RPC API.
#[rpc(server, client, namespace = "evolve")]
pub trait EvolveProposerApi {
    /// Returns the next proposer stored by the proposer-control precompile.
    #[method(name = "getNextProposer")]
    async fn get_next_proposer(&self, block: Option<BlockNumberOrTag>) -> RpcResult<B256>;
}

/// Implementation of the Evolve proposer-control RPC API.
#[derive(Debug, Clone)]
pub struct EvolveProposerApiImpl<Provider> {
    provider: Provider,
    initial_next_proposer: B256,
}

impl<Provider> EvolveProposerApiImpl<Provider> {
    /// Creates a new proposer-control API.
    pub const fn new(provider: Provider, initial_next_proposer: B256) -> Self {
        Self {
            provider,
            initial_next_proposer,
        }
    }

    fn rpc_error(message: impl Into<String>) -> ErrorObjectOwned {
        ErrorObjectOwned::owned(INTERNAL_ERROR, message.into(), None::<()>)
    }
}

#[async_trait]
impl<Provider> EvolveProposerApiServer for EvolveProposerApiImpl<Provider>
where
    Provider: StateProviderFactory + Send + Sync + 'static,
{
    async fn get_next_proposer(&self, block: Option<BlockNumberOrTag>) -> RpcResult<B256> {
        let block = block.unwrap_or(BlockNumberOrTag::Latest);
        let state = self
            .provider
            .state_by_block_number_or_tag(block)
            .map_err(|err| Self::rpc_error(format!("failed to load state for {block:?}: {err}")))?;
        let value = state
            .storage(
                ev_revm::PROPOSER_CONTROL_PRECOMPILE_ADDR,
                NEXT_PROPOSER_SLOT,
            )
            .map_err(|err| {
                Self::rpc_error(format!("failed to read proposer control storage: {err}"))
            })?
            .unwrap_or_default();

        // A zero slot means "never rotated", so fall back to the configured initial proposer.
        // This is applied for every block, including blocks before the precompile's activation
        // height: `initialNextProposer` is documented as the currently active proposer, so
        // pre-activation queries stay stable across the activation boundary instead of
        // reporting zero.
        if value.is_zero() {
            Ok(self.initial_next_proposer)
        } else {
            Ok(B256::from(U256::from(value)))
        }
    }
}
