use crate::{config::EvolvePayloadBuilderConfig, executor::EvEvmConfig};
use alloy_consensus::transaction::{Transaction, TxHashRef};
use alloy_primitives::Address;
use ev_revm::EvTxEvmFactory;
use evolve_ev_reth::EvolvePayloadAttributes;
use reth_chainspec::{ChainSpec, ChainSpecProvider};
use reth_errors::RethError;
use reth_evm::{
    execute::{BlockBuilder, BlockBuilderOutcome},
    ConfigureEvm, NextBlockEnvAttributes,
};
use reth_payload_builder_primitives::PayloadBuilderError;
use reth_primitives::{transaction::SignedTransaction, Header, SealedHeader};
use reth_primitives_traits::SealedBlock;
use reth_provider::{HeaderProvider, StateProviderFactory};
use reth_revm::{database::StateProviderDatabase, State};
use std::sync::Arc;
use tracing::{debug, debug_span, info, instrument};

type EvolveEthEvmConfig = EvEvmConfig<ChainSpec, EvTxEvmFactory>;

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
    #[instrument(skip(self, attributes), fields(
        parent_hash = %attributes.parent_hash,
        tx_count = attributes.transactions.len(),
        gas_limit = ?attributes.gas_limit,
    ))]
    pub async fn build_payload(
        &self,
        attributes: EvolvePayloadAttributes,
    ) -> Result<SealedBlock<ev_primitives::Block>, PayloadBuilderError> {
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
            .header(attributes.parent_hash)
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
            extra_data: Default::default(),
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
        info!(tx_count = attributes.transactions.len(), "executing transactions");
        for (i, tx) in attributes.transactions.iter().enumerate() {
            let _span = debug_span!("execute_tx",
                index = i,
                hash = %tx.tx_hash(),
                nonce = tx.nonce(),
                gas_limit = tx.gas_limit(),
            )
            .entered();

            let recovered_tx = tx.try_clone_into_recovered().map_err(|_| {
                PayloadBuilderError::Internal(RethError::Other(
                    "Failed to recover transaction".into(),
                ))
            })?;

            match builder.execute_transaction(recovered_tx) {
                Ok(gas_used) => {
                    debug!(gas_used, "transaction executed successfully");
                }
                Err(err) => {
                    tracing::warn!(error = ?err, "transaction execution failed");
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

        info!(
            block_number = sealed_block.number,
            block_hash = ?sealed_block.hash(),
            tx_count = sealed_block.transaction_count(),
            gas_used = sealed_block.gas_used,
            "built block"
        );

        // Return the sealed block
        Ok(sealed_block)
    }
}

/// Creates a new payload builder service.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EvolvePayloadBuilderConfig;
    use crate::executor::EvolveEvmConfig;
    use crate::test_utils::SpanCollector;
    use alloy_primitives::B256;
    use evolve_ev_reth::EvolvePayloadAttributes;
    use reth_chainspec::ChainSpecBuilder;
    use reth_primitives::Header;
    use reth_provider::test_utils::MockEthProvider;

    #[tokio::test]
    async fn build_payload_span_has_expected_fields() {
        let collector = SpanCollector::new();
        let _guard = collector.as_default();

        let genesis: alloy_genesis::Genesis =
            serde_json::from_str(include_str!("../../tests/assets/genesis.json"))
                .expect("valid genesis");
        let chain_spec = Arc::new(
            ChainSpecBuilder::default()
                .chain(reth_chainspec::Chain::from_id(1234))
                .genesis(genesis)
                .cancun_activated()
                .build(),
        );

        let provider = MockEthProvider::default();
        let genesis_hash =
            B256::from_slice(&hex::decode("2b8bbb1ea1e04f9c9809b4b278a8687806edc061a356c7dbc491930d8e922503").unwrap());
        let genesis_state_root =
            B256::from_slice(&hex::decode("05e9954443da80d86f2104e56ffdfd98fe21988730684360104865b3dc8191b4").unwrap());

        let genesis_header = Header {
            state_root: genesis_state_root,
            number: 0,
            gas_limit: 30_000_000,
            timestamp: 1710338135,
            base_fee_per_gas: Some(0),
            excess_blob_gas: Some(0),
            blob_gas_used: Some(0),
            parent_beacon_block_root: Some(B256::ZERO),
            ..Default::default()
        };
        provider.add_header(genesis_hash, genesis_header);

        let config =
            EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).unwrap();
        let evm_config = EvolveEvmConfig::new(chain_spec);
        let builder =
            EvolvePayloadBuilder::new(Arc::new(provider), evm_config, config);

        let attributes = EvolvePayloadAttributes::new(
            vec![],
            Some(30_000_000),
            1710338136,
            B256::random(),
            Address::random(),
            genesis_hash,
            1,
        );

        // we only care that the span was created with the right fields,
        // not whether the payload build itself succeeds.
        let _ = builder.build_payload(attributes).await;

        let span = collector
            .find_span("build_payload")
            .expect("build_payload span should be recorded");

        assert!(span.has_field("parent_hash"), "span missing parent_hash field");
        assert!(span.has_field("tx_count"), "span missing tx_count field");
        assert!(span.has_field("gas_limit"), "span missing gas_limit field");
    }

    #[tokio::test]
    async fn execute_tx_span_has_expected_fields() {
        use alloy_consensus::TxLegacy;
        use alloy_primitives::{Bytes, ChainId, Signature, TxKind, U256};
        use ev_primitives::EvTxEnvelope;

        let collector = SpanCollector::new();
        let _guard = collector.as_default();

        let genesis: alloy_genesis::Genesis =
            serde_json::from_str(include_str!("../../tests/assets/genesis.json"))
                .expect("valid genesis");
        let chain_spec = Arc::new(
            ChainSpecBuilder::default()
                .chain(reth_chainspec::Chain::from_id(1234))
                .genesis(genesis)
                .cancun_activated()
                .build(),
        );

        let provider = MockEthProvider::default();
        let genesis_hash =
            B256::from_slice(&hex::decode("2b8bbb1ea1e04f9c9809b4b278a8687806edc061a356c7dbc491930d8e922503").unwrap());
        let genesis_state_root =
            B256::from_slice(&hex::decode("05e9954443da80d86f2104e56ffdfd98fe21988730684360104865b3dc8191b4").unwrap());

        let genesis_header = Header {
            state_root: genesis_state_root,
            number: 0,
            gas_limit: 30_000_000,
            timestamp: 1710338135,
            base_fee_per_gas: Some(0),
            excess_blob_gas: Some(0),
            blob_gas_used: Some(0),
            parent_beacon_block_root: Some(B256::ZERO),
            ..Default::default()
        };
        provider.add_header(genesis_hash, genesis_header);

        let config =
            EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).unwrap();
        let evm_config = EvolveEvmConfig::new(chain_spec);
        let builder =
            EvolvePayloadBuilder::new(Arc::new(provider), evm_config, config);

        let legacy_tx = TxLegacy {
            chain_id: Some(ChainId::from(1234u64)),
            nonce: 0,
            gas_price: 0,
            gas_limit: 21_000,
            to: TxKind::Call(Address::ZERO),
            value: U256::ZERO,
            input: Bytes::default(),
        };
        let signed = alloy_consensus::Signed::new_unhashed(
            reth_primitives::Transaction::Legacy(legacy_tx),
            Signature::test_signature(),
        );
        let tx = EvTxEnvelope::Ethereum(
            reth_ethereum_primitives::TransactionSigned::from(signed),
        );

        let attributes = EvolvePayloadAttributes::new(
            vec![tx],
            Some(30_000_000),
            1710338136,
            B256::random(),
            Address::random(),
            genesis_hash,
            1,
        );

        let _ = builder.build_payload(attributes).await;

        let span = collector
            .find_span("execute_tx")
            .expect("execute_tx span should be recorded");

        assert!(span.has_field("index"), "span missing index field");
        assert!(span.has_field("hash"), "span missing hash field");
        assert!(span.has_field("nonce"), "span missing nonce field");
        assert!(span.has_field("gas_limit"), "span missing gas_limit field");
    }
}
