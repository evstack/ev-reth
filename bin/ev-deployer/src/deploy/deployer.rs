//! `ChainDeployer` trait and `LiveDeployer` implementation.

use crate::deploy::create2::{build_factory_calldata, DETERMINISTIC_DEPLOYER};
use alloy::{
    network::EthereumWallet,
    providers::{Provider, ProviderBuilder},
};
use alloy_primitives::{Address, Bytes, B256};
use alloy_rpc_types_eth::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use async_trait::async_trait;

/// Receipt from a confirmed transaction.
#[derive(Debug)]
pub struct TxReceipt {
    pub tx_hash: B256,
    pub success: bool,
}

/// Abstracts on-chain operations for the deploy pipeline.
#[async_trait]
pub trait ChainDeployer: Send + Sync {
    /// Get the chain ID of the connected chain.
    async fn chain_id(&self) -> eyre::Result<u64>;

    /// Read the bytecode at an address. Returns empty bytes if no code.
    async fn get_code(&self, address: Address) -> eyre::Result<Bytes>;

    /// Send a CREATE2 deployment transaction via the deterministic deployer.
    /// Returns the tx hash once the tx is confirmed.
    async fn deploy_create2(&self, salt: B256, initcode: &[u8]) -> eyre::Result<TxReceipt>;
}

/// Live deployer using alloy provider + signer.
pub struct LiveDeployer {
    provider: Box<dyn Provider>,
}

impl LiveDeployer {
    /// Create a new `LiveDeployer` from an RPC URL and a hex-encoded private key.
    pub fn new(rpc_url: &str, private_key_hex: &str) -> eyre::Result<Self> {
        let key_hex = private_key_hex
            .strip_prefix("0x")
            .unwrap_or(private_key_hex);
        let signer: PrivateKeySigner = key_hex.parse()?;
        let wallet = EthereumWallet::from(signer);

        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(rpc_url.parse()?);

        Ok(Self {
            provider: Box::new(provider),
        })
    }
}

#[async_trait]
impl ChainDeployer for LiveDeployer {
    async fn chain_id(&self) -> eyre::Result<u64> {
        Ok(self.provider.get_chain_id().await?)
    }

    async fn get_code(&self, address: Address) -> eyre::Result<Bytes> {
        Ok(self.provider.get_code_at(address).await?)
    }

    async fn deploy_create2(&self, salt: B256, initcode: &[u8]) -> eyre::Result<TxReceipt> {
        let calldata = build_factory_calldata(salt, initcode);

        let tx = TransactionRequest::default()
            .to(DETERMINISTIC_DEPLOYER)
            .input(calldata.into());

        let pending = self.provider.send_transaction(tx).await?;
        let receipt = pending.get_receipt().await?;

        Ok(TxReceipt {
            tx_hash: receipt.transaction_hash,
            success: receipt.status(),
        })
    }
}
