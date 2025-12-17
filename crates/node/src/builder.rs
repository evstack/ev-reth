use crate::config::EvolvePayloadBuilderConfig;
use alloy_consensus::transaction::Transaction;
use alloy_evm::eth::EthEvmFactory;
use alloy_primitives::Address;
use ev_revm::EvEvmFactory;
use evolve_ev_reth::EvolvePayloadAttributes;
use reth_chainspec::{ChainSpec, ChainSpecProvider};
use reth_errors::RethError;
use reth_evm::{
    execute::{BlockBuilder, BlockBuilderOutcome},
    ConfigureEvm, NextBlockEnvAttributes,
};
use reth_evm_ethereum::EthEvmConfig;
use reth_payload_builder_primitives::PayloadBuilderError;
use reth_primitives::{transaction::SignedTransaction, Header, SealedBlock, SealedHeader};
use reth_provider::{HeaderProvider, StateProviderFactory};
use reth_revm::{database::StateProviderDatabase, State};
use std::sync::Arc;
use tracing::{debug, info};

type EvolveEthEvmConfig = EthEvmConfig<ChainSpec, EvEvmFactory<EthEvmFactory>>;

/// Payload builder for Evolve Reth node
#[derive(Debug)]
pub struct EvolvePayloadBuilder<Client> {
    /// The client for state access
    pub client: Arc<Client>,
    /// EVM configuration (potentially wrapped with base fee redirect)
    pub evm_config: EvolveEthEvmConfig,
    /// Parsed Evolve-specific configuration
    pub config: EvolvePayloadBuilderConfig,
}

impl<Client> EvolvePayloadBuilder<Client>
where
    Client: StateProviderFactory
        + HeaderProvider<Header = Header>
        + ChainSpecProvider<ChainSpec = ChainSpec>
        + Send
        + Sync
        + 'static,
{
    /// Creates a new instance of `EvolvePayloadBuilder`
    pub fn new(
        client: Arc<Client>,
        evm_config: EvolveEthEvmConfig,
        config: EvolvePayloadBuilderConfig,
    ) -> Self {
        if let Some((sink, activation)) = config.base_fee_redirect_settings() {
            info!(
                target: "ev-reth",
                fee_sink = ?sink,
                activation_height = activation,
                "Base fee redirect enabled via chainspec"
            );
        }

        Self {
            client,
            evm_config,
            config,
        }
    }

    /// Builds a payload using the provided attributes
    pub async fn build_payload(
        &self,
        attributes: EvolvePayloadAttributes,
    ) -> Result<SealedBlock, PayloadBuilderError> {
        // Validate attributes
        attributes
            .validate()
            .map_err(|e| PayloadBuilderError::Internal(RethError::Other(Box::new(e))))?;

        // Get the latest state provider
        let state_provider = self.client.latest().map_err(PayloadBuilderError::other)?;

        // Create a database from the state provider
        let db = StateProviderDatabase::new(&state_provider);
        let mut state_db = State::builder()
            .with_database(db)
            .with_bundle_update()
            .build();

        // Get parent header using the client's HeaderProvider trait
        let parent_header = self
            .client
            .header(&attributes.parent_hash)
            .map_err(PayloadBuilderError::other)?
            .ok_or_else(|| {
                PayloadBuilderError::Internal(RethError::Other("Parent header not found".into()))
            })?;
        let block_number = parent_header.number + 1;
        let sealed_parent = SealedHeader::new(parent_header, attributes.parent_hash);

        // Create next block environment attributes
        let gas_limit = attributes.gas_limit.ok_or_else(|| {
            PayloadBuilderError::Internal(RethError::Other(
                "Gas limit is required for evolve payloads".into(),
            ))
        })?;

        // Set coinbase/beneficiary from attributes, defaulting to sink when unset.
        let mut suggested_fee_recipient = attributes.suggested_fee_recipient;
        if suggested_fee_recipient == Address::ZERO {
            if let Some(sink) = self.config.base_fee_sink_for_block(block_number) {
                suggested_fee_recipient = sink;
                info!(
                    target: "ev-reth",
                    fee_sink = ?sink,
                    block_number,
                    "Suggested fee recipient missing; defaulting to base-fee sink"
                );
            }
        }

        let next_block_attrs = NextBlockEnvAttributes {
            timestamp: attributes.timestamp,
            suggested_fee_recipient,
            prev_randao: attributes.prev_randao,
            gas_limit,
            parent_beacon_block_root: Some(alloy_primitives::B256::ZERO), // Set to zero for evolve blocks
            // For post-Shanghai/Cancun chains, an empty withdrawals list is valid
            // and ensures version-specific fields are initialized.
            withdrawals: Some(Default::default()),
        };

        let mut builder = self
            .evm_config
            .builder_for_next_block(&mut state_db, &sealed_parent, next_block_attrs)
            .map_err(PayloadBuilderError::other)?;

        // Apply pre-execution changes
        builder
            .apply_pre_execution_changes()
            .map_err(|err| PayloadBuilderError::Internal(err.into()))?;

        // Execute transactions
        tracing::info!(
            transaction_count = attributes.transactions.len(),
            "Evolve payload builder: executing transactions"
        );
        for (i, tx) in attributes.transactions.iter().enumerate() {
            tracing::debug!(
            index = i,
            hash = ?tx.hash(),
            nonce = tx.nonce(),
            gas_price = ?tx.gas_price(),
            gas_limit = tx.gas_limit(),
            "Processing transaction"
            );

            // Convert to recovered transaction for execution
            let recovered_tx = tx.try_clone_into_recovered().map_err(|_| {
                PayloadBuilderError::Internal(RethError::Other(
                    "Failed to recover transaction".into(),
                ))
            })?;

            // Execute the transaction
            match builder.execute_transaction(recovered_tx) {
                Ok(gas_used) => {
                    tracing::debug!(index = i, gas_used, "Transaction executed successfully");
                    debug!(
                        "[debug] execute_transaction ok: index={}, gas_used={}",
                        i, gas_used
                    );
                }
                Err(err) => {
                    // Log the error but continue with other transactions
                    tracing::warn!(index = i, error = ?err, "Transaction execution failed");
                    debug!(
                        "[debug] execute_transaction err: index={}, err={:?}",
                        i, err
                    );
                }
            }
        }

        // Finish building the block - this calculates the proper state root
        let BlockBuilderOutcome {
            execution_result: _,
            hashed_state: _,
            trie_updates: _,
            block,
        } = builder
            .finish(&state_provider)
            .map_err(PayloadBuilderError::other)?;

        let sealed_block = block.sealed_block().clone();

        tracing::info!(
                    block_number = sealed_block.number,
                    block_hash = ?sealed_block.hash(),
                    transaction_count = sealed_block.transaction_count(),
                    gas_used = sealed_block.gas_used,
                    "Evolve payload builder: built block"
        );

        // Return the sealed block
        Ok(sealed_block)
    }
}

/// Creates a new payload builder service
pub fn create_payload_builder_service<Client>(
    client: Arc<Client>,
    evm_config: EvolveEthEvmConfig,
) -> Option<EvolvePayloadBuilder<Client>>
where
    Client: StateProviderFactory
        + HeaderProvider<Header = Header>
        + ChainSpecProvider<ChainSpec = ChainSpec>
        + Send
        + Sync
        + 'static,
{
    let chain_spec = client.chain_spec();

    let config = match EvolvePayloadBuilderConfig::from_chain_spec(&chain_spec) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(target: "ev-reth", error = ?err, "Failed to parse chainspec extras");
            return None;
        }
    };

    if let Err(err) = config.validate() {
        tracing::warn!(target: "ev-reth", error = ?err, "Invalid evolve payload builder configuration");
        return None;
    }

    Some(EvolvePayloadBuilder::new(client, evm_config, config))
}
