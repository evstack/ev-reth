use std::sync::Arc;

use alloy_consensus::{
    constants::EIP1559_TX_TYPE_ID,
    transaction::{Recovered, TxHashRef},
    BlobTransactionValidationError, Signed, Typed2718,
};
use alloy_eips::{
    eip2718::{Encodable2718, WithEncoded},
    eip7594::BlobTransactionSidecarVariant,
    eip7840::BlobParams,
    merge::EPOCH_SLOTS,
};
use alloy_primitives::{Address, U256};
use c_kzg::KzgSettings;
use ev_primitives::{EvNodeTransaction, EvPooledTxEnvelope, EvTxEnvelope, TransactionSigned};
use reth_chainspec::{ChainSpecProvider, EthChainSpec, EthereumHardforks};
use reth_node_api::{FullNodeTypes, NodeTypes};
use reth_node_builder::{
    components::{create_blob_store_with_cache, PoolBuilder, TxPoolBuilder},
    BuilderContext,
};
use reth_primitives_traits::NodePrimitives;
use reth_storage_api::{AccountInfoReader, BlockNumReader, StateProviderFactory};
use reth_transaction_pool::{
    blobstore::DiskFileBlobStore,
    error::{InvalidPoolTransactionError, PoolTransactionError},
    CoinbaseTipOrdering, EthBlobTransactionSidecar, EthPoolTransaction, EthPooledTransaction,
    EthTransactionValidator, PoolTransaction, TransactionOrigin, TransactionValidationOutcome,
    TransactionValidationTaskExecutor, TransactionValidator,
};
use tracing::{debug, info, instrument, warn};

/// Pool transaction wrapper for `EvTxEnvelope`.
#[derive(Debug, Clone)]
pub struct EvPooledTransaction {
    inner: EthPooledTransaction<EvTxEnvelope>,
}

impl EvPooledTransaction {
    /// Creates a new pooled transaction from a recovered envelope and encoded length.
    pub fn new(transaction: Recovered<EvTxEnvelope>, encoded_length: usize) -> Self {
        Self {
            inner: EthPooledTransaction::new(transaction, encoded_length),
        }
    }

    /// Returns the recovered transaction.
    pub const fn transaction(&self) -> &Recovered<EvTxEnvelope> {
        self.inner.transaction()
    }
}

impl PoolTransaction for EvPooledTransaction {
    type TryFromConsensusError =
        alloy_consensus::error::ValueError<reth_ethereum_primitives::TransactionSigned>;
    type Consensus = EvTxEnvelope;
    type Pooled = EvPooledTxEnvelope;

    fn clone_into_consensus(&self) -> Recovered<Self::Consensus> {
        self.inner.transaction().clone()
    }

    fn into_consensus(self) -> Recovered<Self::Consensus> {
        self.inner.transaction
    }

    fn into_consensus_with2718(self) -> WithEncoded<Recovered<Self::Consensus>> {
        self.inner.transaction.into_encoded()
    }

    fn from_pooled(tx: Recovered<Self::Pooled>) -> Self {
        let encoded_length = tx.encode_2718_len();
        let (tx, signer) = tx.into_parts();
        match tx {
            EvPooledTxEnvelope::Ethereum(tx) => match tx {
                reth_ethereum_primitives::PooledTransactionVariant::Eip4844(tx) => {
                    let (tx, sig, hash) = tx.into_parts();
                    let (tx, blob) = tx.into_parts();
                    let tx = Signed::new_unchecked(tx, sig, hash);
                    let tx = reth_ethereum_primitives::TransactionSigned::from(tx);
                    let tx = EvTxEnvelope::Ethereum(tx);
                    let tx = Recovered::new_unchecked(tx, signer);
                    let mut pooled = Self::new(tx, encoded_length);
                    pooled.inner.blob_sidecar = EthBlobTransactionSidecar::Present(blob);
                    pooled
                }
                tx => {
                    let tx = EvTxEnvelope::Ethereum(tx.into());
                    let tx = Recovered::new_unchecked(tx, signer);
                    Self::new(tx, encoded_length)
                }
            },
            EvPooledTxEnvelope::EvNode(tx) => {
                let tx = EvTxEnvelope::EvNode(tx);
                let tx = Recovered::new_unchecked(tx, signer);
                Self::new(tx, encoded_length)
            }
        }
    }

    fn hash(&self) -> &alloy_primitives::TxHash {
        self.inner.transaction.tx_hash()
    }

    fn sender(&self) -> Address {
        self.inner.transaction.signer()
    }

    fn sender_ref(&self) -> &Address {
        self.inner.transaction.signer_ref()
    }

    fn cost(&self) -> &U256 {
        &self.inner.cost
    }

    fn encoded_length(&self) -> usize {
        self.inner.encoded_length
    }
}

impl Typed2718 for EvPooledTransaction {
    fn ty(&self) -> u8 {
        match self.transaction().inner() {
            // Treat EvNode txs as EIP-1559 for pool validation compatibility.
            EvTxEnvelope::EvNode(_) => EIP1559_TX_TYPE_ID,
            _ => self.inner.ty(),
        }
    }
}

impl reth_primitives_traits::InMemorySize for EvPooledTransaction {
    fn size(&self) -> usize {
        self.inner.size()
    }
}

impl alloy_consensus::Transaction for EvPooledTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        self.inner.chain_id()
    }

    fn nonce(&self) -> u64 {
        self.inner.nonce()
    }

    fn gas_limit(&self) -> u64 {
        self.inner.gas_limit()
    }

    fn gas_price(&self) -> Option<u128> {
        self.inner.gas_price()
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.inner.max_fee_per_gas()
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.inner.max_priority_fee_per_gas()
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        self.inner.max_fee_per_blob_gas()
    }

    fn priority_fee_or_price(&self) -> u128 {
        self.inner.priority_fee_or_price()
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        self.inner.effective_gas_price(base_fee)
    }

    fn is_dynamic_fee(&self) -> bool {
        self.inner.is_dynamic_fee()
    }

    fn kind(&self) -> alloy_primitives::TxKind {
        self.inner.kind()
    }

    fn is_create(&self) -> bool {
        self.inner.is_create()
    }

    fn value(&self) -> U256 {
        self.inner.value()
    }

    fn input(&self) -> &alloy_primitives::Bytes {
        self.inner.input()
    }

    fn access_list(&self) -> Option<&alloy_eips::eip2930::AccessList> {
        self.inner.access_list()
    }

    fn blob_versioned_hashes(&self) -> Option<&[alloy_primitives::B256]> {
        self.inner.blob_versioned_hashes()
    }

    fn authorization_list(&self) -> Option<&[alloy_eips::eip7702::SignedAuthorization]> {
        self.inner.authorization_list()
    }
}

impl EthPoolTransaction for EvPooledTransaction {
    fn take_blob(&mut self) -> EthBlobTransactionSidecar {
        if self.is_eip4844() {
            std::mem::replace(
                &mut self.inner.blob_sidecar,
                EthBlobTransactionSidecar::Missing,
            )
        } else {
            EthBlobTransactionSidecar::None
        }
    }

    fn try_into_pooled_eip4844(
        self,
        sidecar: std::sync::Arc<BlobTransactionSidecarVariant>,
    ) -> Option<Recovered<Self::Pooled>> {
        let (signed_transaction, signer) = self.into_consensus().into_parts();
        match signed_transaction {
            EvTxEnvelope::Ethereum(tx) => {
                let pooled_transaction = tx
                    .try_into_pooled_eip4844(std::sync::Arc::unwrap_or_clone(sidecar))
                    .ok()?;
                Some(Recovered::new_unchecked(
                    EvPooledTxEnvelope::Ethereum(pooled_transaction),
                    signer,
                ))
            }
            EvTxEnvelope::EvNode(_) => None,
        }
    }

    fn try_from_eip4844(
        tx: Recovered<Self::Consensus>,
        sidecar: BlobTransactionSidecarVariant,
    ) -> Option<Self> {
        let (tx, signer) = tx.into_parts();
        match tx {
            EvTxEnvelope::Ethereum(tx) => tx
                .try_into_pooled_eip4844(sidecar)
                .ok()
                .map(|tx| Recovered::new_unchecked(EvPooledTxEnvelope::Ethereum(tx), signer))
                .map(Self::from_pooled),
            EvTxEnvelope::EvNode(_) => None,
        }
    }

    fn validate_blob(
        &self,
        sidecar: &BlobTransactionSidecarVariant,
        settings: &KzgSettings,
    ) -> Result<(), BlobTransactionValidationError> {
        match self.inner.transaction.inner() {
            EvTxEnvelope::Ethereum(tx) => match tx.as_eip4844() {
                Some(tx) => tx.tx().validate_blob(sidecar, settings),
                None => Err(BlobTransactionValidationError::NotBlobTransaction(
                    self.ty(),
                )),
            },
            EvTxEnvelope::EvNode(_) => Err(BlobTransactionValidationError::NotBlobTransaction(
                self.ty(),
            )),
        }
    }
}

/// Errors returned by EV-specific transaction pool validation.
#[derive(Debug, thiserror::Error)]
pub enum EvTxPoolError {
    /// `EvNode` transaction must include at least one call.
    #[error("evnode transaction must include at least one call")]
    EmptyCalls,
    /// Only the first call may be a CREATE.
    #[error("only the first call may be CREATE")]
    InvalidCreatePosition,
    /// Sponsor signature failed verification.
    #[error("invalid sponsor signature")]
    InvalidSponsorSignature,
    /// Error while querying account info from the state provider.
    #[error("state provider error: {0}")]
    StateProvider(String),
    /// Top-level contract deployment not allowed for caller.
    #[error("contract deployment not allowed")]
    DeployNotAllowed,
}

impl PoolTransactionError for EvTxPoolError {
    fn is_bad_transaction(&self) -> bool {
        matches!(
            self,
            Self::EmptyCalls
                | Self::InvalidCreatePosition
                | Self::InvalidSponsorSignature
                | Self::DeployNotAllowed
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Transaction validator that adds EV-specific checks on top of the base validator.
#[derive(Debug, Clone)]
pub struct EvTransactionValidator<Client, Evm> {
    inner: Arc<EthTransactionValidator<Client, EvPooledTransaction, Evm>>,
    deploy_allowlist: Option<ev_revm::deploy::DeployAllowlistSettings>,
}

impl<Client, Evm> EvTransactionValidator<Client, Evm>
where
    Client: BlockNumReader,
{
    /// Wraps the provided Ethereum validator with EV-specific validation logic.
    pub fn new(
        inner: EthTransactionValidator<Client, EvPooledTransaction, Evm>,
        deploy_allowlist: Option<ev_revm::deploy::DeployAllowlistSettings>,
    ) -> Self {
        Self {
            inner: Arc::new(inner),
            deploy_allowlist,
        }
    }

    fn check_sender_overdraft(
        pooled: &EvPooledTransaction,
        sender_balance: U256,
    ) -> Result<(), InvalidPoolTransactionError> {
        if sender_balance < *pooled.cost() {
            return Err(InvalidPoolTransactionError::Overdraft {
                cost: *pooled.cost(),
                balance: sender_balance,
            });
        }
        Ok(())
    }

    fn validate_evnode_calls(
        &self,
        tx: &EvNodeTransaction,
    ) -> Result<(), InvalidPoolTransactionError> {
        if tx.calls.is_empty() {
            return Err(InvalidPoolTransactionError::other(
                EvTxPoolError::EmptyCalls,
            ));
        }
        if tx.calls.iter().skip(1).any(|call| call.to.is_create()) {
            return Err(InvalidPoolTransactionError::other(
                EvTxPoolError::InvalidCreatePosition,
            ));
        }
        Ok(())
    }

    fn ensure_state(
        &self,
        state: &mut Option<Box<dyn AccountInfoReader + Send>>,
    ) -> Result<(), InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        if state.is_none() {
            let new_state =
                self.inner
                    .client()
                    .latest()
                    .map_err(|err: reth_provider::ProviderError| {
                        InvalidPoolTransactionError::other(EvTxPoolError::StateProvider(
                            err.to_string(),
                        ))
                    })?;
            *state = Some(Box::new(new_state));
        }
        Ok(())
    }

    fn validate_sponsor_balance(
        &self,
        state: &mut Option<Box<dyn AccountInfoReader + Send>>,
        sponsor: Address,
        gas_cost: U256,
    ) -> Result<U256, InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        self.ensure_state(state)?;
        let state = state.as_ref().expect("state provider is set");
        let account = state
            .basic_account(&sponsor)
            .map_err(|err| {
                InvalidPoolTransactionError::other(EvTxPoolError::StateProvider(err.to_string()))
            })?
            .unwrap_or_default();
        if account.balance < gas_cost {
            return Err(InvalidPoolTransactionError::Overdraft {
                cost: gas_cost,
                balance: account.balance,
            });
        }
        Ok(account.balance)
    }

    /// Validates an EvNode transaction. Returns an optional override balance
    /// for sponsored transactions (the sponsor's balance), so the pool uses
    /// the sponsor's balance for pending/queued ordering instead of the executor's.
    fn validate_evnode(
        &self,
        pooled: &EvPooledTransaction,
        sender_balance: U256,
        state: &mut Option<Box<dyn AccountInfoReader + Send>>,
    ) -> Result<Option<U256>, InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        // Unified deploy allowlist check (covers both Ethereum and EvNode txs).
        if let Some(settings) = &self.deploy_allowlist {
            let is_top_level_create = match pooled.transaction().inner() {
                EvTxEnvelope::Ethereum(tx) => alloy_consensus::Transaction::is_create(tx),
                EvTxEnvelope::EvNode(ref signed) => {
                    let tx = signed.tx();
                    tx.calls.first().map(|c| c.to.is_create()).unwrap_or(false)
                }
            };
            let caller = pooled.transaction().signer();
            let block_number = self.inner.client().best_block_number().map_err(
                |err: reth_provider::ProviderError| {
                    InvalidPoolTransactionError::other(EvTxPoolError::StateProvider(
                        err.to_string(),
                    ))
                },
            )?;
            if let Err(_e) = ev_revm::deploy::check_deploy_allowed(
                Some(settings),
                caller,
                is_top_level_create,
                block_number,
            ) {
                return Err(InvalidPoolTransactionError::other(
                    EvTxPoolError::DeployNotAllowed,
                ));
            }
        }

        let consensus = pooled.transaction().inner();
        let EvTxEnvelope::EvNode(tx) = consensus else {
            Self::check_sender_overdraft(pooled, sender_balance)?;
            return Ok(None);
        };

        let tx = tx.tx();
        self.validate_evnode_calls(tx)?;

        if let Some(signature) = tx.fee_payer_signature.as_ref() {
            // Sponsored transaction: validate sponsor balance and return it
            // so the pool uses the sponsor's balance for pending/queued ordering
            let executor = pooled.transaction().signer();
            let sponsor = tx.recover_sponsor(executor, signature).map_err(|_| {
                InvalidPoolTransactionError::other(EvTxPoolError::InvalidSponsorSignature)
            })?;

            let gas_cost = U256::from(tx.max_fee_per_gas).saturating_mul(U256::from(tx.gas_limit));
            let sponsor_balance = self.validate_sponsor_balance(state, sponsor, gas_cost)?;
            Ok(Some(sponsor_balance))
        } else {
            Self::check_sender_overdraft(pooled, sender_balance)?;
            Ok(None)
        }
    }
}

impl<Client, Evm> TransactionValidator for EvTransactionValidator<Client, Evm>
where
    Client: ChainSpecProvider<ChainSpec: EthereumHardforks> + StateProviderFactory + BlockNumReader,
    Evm: reth_evm::ConfigureEvm + 'static,
{
    type Transaction = EvPooledTransaction;
    type Block = <Evm::Primitives as reth_primitives_traits::NodePrimitives>::Block;

    #[instrument(skip(self, transaction), fields(
        origin = ?origin,
        tx_hash = %transaction.hash(),
        duration_ms = tracing::field::Empty,
    ))]
    async fn validate_transaction(
        &self,
        origin: TransactionOrigin,
        transaction: <Self as TransactionValidator>::Transaction,
    ) -> TransactionValidationOutcome<Self::Transaction> {
        let _start = std::time::Instant::now();
        let mut state = None;
        let outcome = self
            .inner
            .validate_one_with_state(origin, transaction, &mut state);

        let result = match outcome {
            TransactionValidationOutcome::Valid {
                balance,
                state_nonce,
                bytecode_hash,
                transaction,
                propagate,
                authorities,
            } => match self.validate_evnode(transaction.transaction(), balance, &mut state) {
                Ok(override_balance) => TransactionValidationOutcome::Valid {
                    balance: override_balance.unwrap_or(balance),
                    state_nonce,
                    bytecode_hash,
                    transaction,
                    propagate,
                    authorities,
                },
                Err(err) => {
                    TransactionValidationOutcome::Invalid(transaction.into_transaction(), err)
                }
            },
            other => other,
        };

        tracing::Span::current().record("duration_ms", _start.elapsed().as_millis() as u64);
        result
    }
}

/// Pool builder that wires the custom `EvNode` transaction validator.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EvolvePoolBuilder;

impl<Types, Node, Evm> PoolBuilder<Node, Evm> for EvolvePoolBuilder
where
    Types: NodeTypes<
        ChainSpec = reth_chainspec::ChainSpec,
        Primitives: NodePrimitives<SignedTx = TransactionSigned>,
    >,
    Node: FullNodeTypes<Types = Types>,
    Evm: reth_evm::ConfigureEvm<Primitives = Types::Primitives> + 'static,
{
    type Pool = reth_transaction_pool::Pool<
        TransactionValidationTaskExecutor<EvTransactionValidator<Node::Provider, Evm>>,
        CoinbaseTipOrdering<EvPooledTransaction>,
        DiskFileBlobStore,
    >;

    async fn build_pool(self, ctx: &BuilderContext<Node>, evm: Evm) -> eyre::Result<Self::Pool> {
        let pool_config = ctx.pool_config();

        let blobs_disabled = ctx.config().txpool.blobpool_max_count == 0;

        let blob_cache_size = if let Some(blob_cache_size) = pool_config.blob_cache_size {
            Some(blob_cache_size)
        } else {
            let current_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            let blob_params = ctx
                .chain_spec()
                .blob_params_at_timestamp(current_timestamp)
                .unwrap_or_else(BlobParams::cancun);

            Some((blob_params.target_blob_count * EPOCH_SLOTS * 2) as u32)
        };

        let blob_store = create_blob_store_with_cache(ctx, blob_cache_size)?;

        let validator = TransactionValidationTaskExecutor::eth_builder(ctx.provider().clone(), evm)
            .set_eip4844(!blobs_disabled)
            .kzg_settings(ctx.kzg_settings()?)
            .with_max_tx_input_bytes(ctx.config().txpool.max_tx_input_bytes)
            .with_local_transactions_config(pool_config.local_transactions_config.clone())
            .set_tx_fee_cap(ctx.config().rpc.rpc_tx_fee_cap)
            .with_max_tx_gas_limit(ctx.config().txpool.max_tx_gas_limit)
            .with_minimum_priority_fee(ctx.config().txpool.minimum_priority_fee)
            // Disable the standard caller balance check - we handle balance validation
            // in EvTransactionValidator::validate_evnode which checks:
            // - Sponsor balance for sponsored EvNode transactions
            // - Sender balance for non-sponsored EvNode and standard Ethereum transactions
            .disable_balance_check()
            .with_additional_tasks(ctx.config().txpool.additional_validation_tasks)
            .build_with_tasks::<EvPooledTransaction, _, _>(
                ctx.task_executor().clone(),
                blob_store.clone(),
            )
            .map(|inner| {
                // Wire deploy-allowlist from chainspec extras into the pool validator.
                let evolve_config = crate::config::EvolvePayloadBuilderConfig::from_chain_spec(
                    ctx.chain_spec().as_ref(),
                )
                .unwrap_or_else(|err| {
                    warn!(
                        target: "reth::cli",
                        "Failed to parse evolve config from chainspec: {err}"
                    );
                    Default::default()
                });
                let deploy_allowlist =
                    evolve_config
                        .deploy_allowlist_settings()
                        .map(|(allowlist, activation)| {
                            ev_revm::deploy::DeployAllowlistSettings::new(allowlist, activation)
                        });
                EvTransactionValidator::new(inner, deploy_allowlist)
            });

        if validator.validator().inner.eip4844() {
            let kzg_settings = validator.validator().inner.kzg_settings().clone();
            ctx.task_executor().spawn_blocking(move || {
                let _ = kzg_settings.get();
                debug!(target: "reth::cli", "Initialized KZG settings");
            });
        }

        let transaction_pool = TxPoolBuilder::new(ctx)
            .with_validator(validator)
            .build_and_spawn_maintenance_task(blob_store, pool_config)?;

        info!(target: "reth::cli", "Transaction pool initialized");
        debug!(target: "reth::cli", "Spawned txpool maintenance task");

        Ok(transaction_pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::Signed;
    use alloy_eips::eip2930::AccessList;
    use alloy_primitives::{Bytes, Signature, TxKind};
    use ev_primitives::{Call, EvNodeSignedTx, EvNodeTransaction};
    use reth_provider::test_utils::MockEthProvider;

    fn sample_signature() -> Signature {
        let mut bytes = [0u8; 65];
        bytes[64] = 27;
        Signature::from_raw_array(&bytes).expect("valid test signature")
    }

    /// Creates a non-sponsored `EvNode` transaction (`fee_payer_signature` = None)
    fn create_non_sponsored_evnode_tx(gas_limit: u64, max_fee_per_gas: u128) -> EvNodeSignedTx {
        let tx = EvNodeTransaction {
            chain_id: 1,
            nonce: 0,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas,
            gas_limit,
            calls: vec![Call {
                to: TxKind::Call(Address::ZERO),
                value: U256::ZERO,
                input: Bytes::new(),
            }],
            access_list: AccessList::default(),
            fee_payer_signature: None, // Non-sponsored
        };
        Signed::new_unhashed(tx, sample_signature())
    }

    /// Creates a non-sponsored `EvNode` transaction with CREATE as the first call.
    fn create_non_sponsored_evnode_create_tx(
        gas_limit: u64,
        max_fee_per_gas: u128,
    ) -> EvNodeSignedTx {
        let tx = EvNodeTransaction {
            chain_id: 1,
            nonce: 0,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas,
            gas_limit,
            calls: vec![Call {
                to: TxKind::Create,
                value: U256::ZERO,
                input: Bytes::from_static(&[0x60, 0x00, 0x60, 0x00, 0xf3]), // minimal initcode
            }],
            access_list: AccessList::default(),
            fee_payer_signature: None,
        };
        Signed::new_unhashed(tx, sample_signature())
    }

    fn create_pooled_tx(signed_tx: EvNodeSignedTx, signer: Address) -> EvPooledTransaction {
        let envelope = EvTxEnvelope::EvNode(signed_tx);
        let recovered = alloy_consensus::transaction::Recovered::new_unchecked(envelope, signer);
        let encoded_length = 200; // Approximate length for test
        EvPooledTransaction::new(recovered, encoded_length)
    }

    fn create_test_validator(
        deploy_allowlist: Option<ev_revm::deploy::DeployAllowlistSettings>,
    ) -> EvTransactionValidator<MockEthProvider, crate::executor::EvolveEvmConfig> {
        use reth_transaction_pool::{
            blobstore::InMemoryBlobStore, validate::EthTransactionValidatorBuilder,
        };
        let provider = MockEthProvider::default().with_genesis_block();
        let evm = crate::executor::EvolveEvmConfig::new(provider.chain_spec());
        let blob_store = InMemoryBlobStore::default();
        let inner = EthTransactionValidatorBuilder::new(provider, evm)
            .no_shanghai()
            .no_cancun()
            .build(blob_store);
        EvTransactionValidator::new(inner, deploy_allowlist)
    }

    /// Tests that non-sponsored `EvNode` transactions with insufficient sender balance
    /// are rejected with an Overdraft error.
    ///
    /// BUG: Currently this test FAILS because `validate_evnode` does not check
    /// sender balance for non-sponsored `EvNode` transactions.
    #[test]
    fn non_sponsored_evnode_rejects_insufficient_balance() {
        let validator = create_test_validator(None);

        // Create a non-sponsored EvNode transaction
        let gas_limit = 21_000u64;
        let max_fee_per_gas = 1_000_000_000u128; // 1 gwei
        let signed_tx = create_non_sponsored_evnode_tx(gas_limit, max_fee_per_gas);

        let signer = Address::random();
        let pooled = create_pooled_tx(signed_tx, signer);

        // Sender has ZERO balance - clearly insufficient
        let sender_balance = U256::ZERO;
        let mut state: Option<Box<dyn AccountInfoReader + Send>> = None;

        // Call validate_evnode - should return Overdraft error
        let result = validator.validate_evnode(&pooled, sender_balance, &mut state);

        assert!(
            result.is_err(),
            "Non-sponsored EvNode with zero balance should be rejected, but got Ok"
        );

        if let Err(err) = result {
            assert!(
                matches!(err, InvalidPoolTransactionError::Overdraft { .. }),
                "Expected Overdraft error, got: {:?}",
                err
            );
        }
    }

    /// Tests that non-sponsored `EvNode` transactions with sufficient balance are accepted.
    #[test]
    fn non_sponsored_evnode_accepts_sufficient_balance() {
        let validator = create_test_validator(None);

        let gas_limit = 21_000u64;
        let max_fee_per_gas = 1_000_000_000u128;
        let signed_tx = create_non_sponsored_evnode_tx(gas_limit, max_fee_per_gas);

        let signer = Address::random();
        let pooled = create_pooled_tx(signed_tx, signer);

        let tx_cost = *pooled.cost();

        // Sender has MORE than enough balance
        let sender_balance = tx_cost + U256::from(1);
        let mut state: Option<Box<dyn AccountInfoReader + Send>> = None;

        let result = validator.validate_evnode(&pooled, sender_balance, &mut state);

        assert!(
            result.is_ok(),
            "Non-sponsored EvNode with sufficient balance should be accepted, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn validate_transaction_span_has_expected_fields() {
        use crate::test_utils::SpanCollector;

        let collector = SpanCollector::new();
        let _guard = collector.as_default();

        let validator = create_test_validator(None);

        let gas_limit = 21_000u64;
        let max_fee_per_gas = 1_000_000_000u128;
        let signed_tx = create_non_sponsored_evnode_tx(gas_limit, max_fee_per_gas);

        let signer = Address::random();
        let pooled = create_pooled_tx(signed_tx, signer);

        let _ = validator
            .validate_transaction(TransactionOrigin::External, pooled)
            .await;

        let span = collector
            .find_span("validate_transaction")
            .expect("validate_transaction span should be recorded");

        assert!(span.has_field("origin"), "span missing origin field");
        assert!(span.has_field("tx_hash"), "span missing tx_hash field");
        assert!(
            span.has_field("duration_ms"),
            "span missing duration_ms field"
        );
    }

    /// Tests pool-level deploy allowlist rejection for `EvNode` CREATE when caller not allowlisted.
    #[test]
    fn evnode_create_rejected_when_not_allowlisted() {
        // Configure deploy allowlist with a different address than the signer
        let allowed = Address::from([0x11u8; 20]);
        let settings = ev_revm::deploy::DeployAllowlistSettings::new(vec![allowed], 0);
        let validator = create_test_validator(Some(settings));

        let gas_limit = 200_000u64;
        let max_fee_per_gas = 1_000_000_000u128;
        let signed_tx = create_non_sponsored_evnode_create_tx(gas_limit, max_fee_per_gas);

        let signer = Address::from([0x22u8; 20]); // not allowlisted
        let pooled = create_pooled_tx(signed_tx, signer);

        let sender_balance = *pooled.cost() + U256::from(1);
        let mut state: Option<Box<dyn AccountInfoReader + Send>> = None;

        let result = validator.validate_evnode(&pooled, sender_balance, &mut state);
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(matches!(err, InvalidPoolTransactionError::Other(_)));
        }
    }
}
