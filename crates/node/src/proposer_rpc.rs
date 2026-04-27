//! RPC accessors for Evolve proposer control state.

use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{Address, B256};
use async_trait::async_trait;
use jsonrpsee_core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use jsonrpsee_types::ErrorObjectOwned;
use reth_storage_api::StateProviderFactory;

const NEXT_PROPOSER_SLOT: B256 = B256::ZERO;
const INTERNAL_ERROR: i32 = -32603;

/// Evolve proposer-control RPC API.
#[rpc(server, namespace = "evolve")]
pub trait EvolveProposerApi {
    /// Returns the next proposer stored by the proposer-control precompile.
    #[method(name = "getNextProposer")]
    async fn get_next_proposer(&self, block: Option<BlockNumberOrTag>) -> RpcResult<Address>;
}

/// Implementation of the Evolve proposer-control RPC API.
#[derive(Debug, Clone)]
pub struct EvolveProposerApiImpl<Provider> {
    provider: Provider,
    initial_next_proposer: Address,
}

impl<Provider> EvolveProposerApiImpl<Provider> {
    /// Creates a new proposer-control API.
    pub const fn new(provider: Provider, initial_next_proposer: Address) -> Self {
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
    async fn get_next_proposer(&self, block: Option<BlockNumberOrTag>) -> RpcResult<Address> {
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

        if value.is_zero() {
            Ok(self.initial_next_proposer)
        } else {
            Ok(Address::from_word(value.into()))
        }
    }
}
