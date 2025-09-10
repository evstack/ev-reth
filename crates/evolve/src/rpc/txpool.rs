use alloy_primitives::Bytes;
use async_trait::async_trait;
use jsonrpsee::tracing::debug;
use jsonrpsee_core::RpcResult;
use jsonrpsee_proc_macros::rpc;
use reth_transaction_pool::{PoolTransaction, TransactionPool};

/// Rollkit txpool RPC API trait
#[rpc(server, namespace = "txpoolExt")]
pub trait RollkitTxpoolApi {
    /// Get transactions from the pool up to the configured `max_bytes` limit
    #[method(name = "getTxs")]
    async fn get_txs(&self) -> RpcResult<Vec<Bytes>>;
}

/// Implementation of the Rollkit txpool RPC API
#[derive(Debug)]
pub struct RollkitTxpoolApiImpl<Pool> {
    /// Transaction pool
    pool: Pool,
    /// Maximum bytes allowed for transaction selection
    max_bytes: u64,
    /// Maximum gas allowed for transaction selection
    max_gas: u64,
}

impl<Pool> RollkitTxpoolApiImpl<Pool> {
    /// Creates a new instance of `TxpoolApi`.
    pub const fn new(pool: Pool, max_bytes: u64, max_gas: u64) -> Self {
        Self {
            pool,
            max_bytes,
            max_gas,
        }
    }
}

/// Creates a new Rollkit txpool RPC module
pub const fn create_rollkit_txpool_module<Pool>(
    pool: Pool,
    max_bytes: u64,
    max_gas: u64,
) -> RollkitTxpoolApiImpl<Pool>
where
    Pool: TransactionPool + Send + Sync + 'static,
{
    RollkitTxpoolApiImpl {
        pool,
        max_bytes,
        max_gas,
    }
}

#[async_trait]
impl<Pool> RollkitTxpoolApiServer for RollkitTxpoolApiImpl<Pool>
where
    Pool: TransactionPool + Send + Sync + 'static,
{
    /// Returns a Geth-style `TxpoolContent` with raw RLP hex strings.
    async fn get_txs(&self) -> RpcResult<Vec<Bytes>> {
        //------------------------------------------------------------------//
        // 1. Iterate best txs (sorted by priority) and stop once we hit    //
        //    the byte cap                                                   //
        //------------------------------------------------------------------//
        let mut total_bytes = 0u64;
        let mut total_gas = 0u64;
        let mut selected_txs: Vec<Bytes> = Vec::new();

        // Use best_transactions() which returns an iterator of transactions
        // ordered by their priority (gas price/priority fee)
        for best_tx in self.pool.best_transactions() {
            // Convert for gas introspection and encoding
            let tx = best_tx.transaction.clone().into_consensus_with2718();

            // Size and gas of this tx
            let sz = best_tx.encoded_length() as u64;
            let gas = best_tx.gas_limit();

            // Enforce byte cap if configured (> 0)
            if self.max_bytes > 0 && total_bytes + sz > self.max_bytes {
                break;
            }
            // Enforce gas cap if configured (> 0)
            if self.max_gas > 0 && total_gas + gas > self.max_gas {
                break;
            }

            let bz = tx.encoded_bytes();
            selected_txs.push(bz.clone());

            total_bytes += sz;
            total_gas += gas;
        }

        debug!(
            "get_txs returning {} transactions ({} bytes, {} gas)",
            selected_txs.len(),
            total_bytes,
            total_gas
        );
        Ok(selected_txs)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{RollkitConfig, DEFAULT_MAX_TXPOOL_BYTES, DEFAULT_MAX_TXPOOL_GAS};

    #[test]
    fn test_default_config_value() {
        // Test that the default max_txpool_bytes value is correctly set
        let config = RollkitConfig::default();
        assert_eq!(config.max_txpool_bytes, DEFAULT_MAX_TXPOOL_BYTES);
        assert_eq!(DEFAULT_MAX_TXPOOL_BYTES, 1_939_865); // 1.85 MiB
                                                         // Test default gas cap is set
        assert_eq!(config.max_txpool_gas, DEFAULT_MAX_TXPOOL_GAS);
    }

    #[test]
    fn test_rollkit_txpool_api_creation() {
        // This test verifies that we can create the API with different max_bytes values
        // The actual behavior testing would require a mock transaction pool

        // Test with default config
        let config = RollkitConfig::default();
        assert_eq!(config.max_txpool_bytes, 1_939_865); // 1.85 MiB

        // Test with custom config
        let custom_config = RollkitConfig::new(1000);
        assert_eq!(custom_config.max_txpool_bytes, 1000);
        // And custom gas using builder
        let custom_with_gas = RollkitConfig::new_with_gas(1000, 1_000_000);
        assert_eq!(custom_with_gas.max_txpool_bytes, 1000);
        assert_eq!(custom_with_gas.max_txpool_gas, 1_000_000);
    }
}
