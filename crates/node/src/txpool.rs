use std::sync::Arc;

use alloy_consensus::{
    transaction::{Recovered, TxHashRef},
    BlobTransactionValidationError, Signed, Typed2718,
};
use alloy_eips::{
    eip2718::Encodable2718,
    eip2718::WithEncoded,
    eip7594::BlobTransactionSidecarVariant,
    eip7840::BlobParams,
    merge::EPOCH_SLOTS,
};
use alloy_primitives::{Address, U256};
use c_kzg::KzgSettings;
use ev_primitives::{EvNodeTransaction, EvPooledTxEnvelope, EvTxEnvelope, TransactionSigned};
use reth_chainspec::{ChainSpecProvider, EthChainSpec, EthereumHardforks};
use reth_node_api::{FullNodeTypes, NodeTypes};
use reth_node_builder::components::{create_blob_store_with_cache, PoolBuilder, TxPoolBuilder};
use reth_node_builder::BuilderContext;
use reth_primitives_traits::NodePrimitives;
use reth_storage_api::{AccountInfoReader, StateProviderFactory};
use reth_transaction_pool::{
    blobstore::DiskFileBlobStore,
    error::{InvalidPoolTransactionError, PoolTransactionError},
    CoinbaseTipOrdering, EthBlobTransactionSidecar, EthPoolTransaction, EthPooledTransaction,
    EthTransactionValidator, PoolTransaction, TransactionOrigin, TransactionValidationOutcome,
    TransactionValidationTaskExecutor, TransactionValidator,
};
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct EvPooledTransaction {
    inner: EthPooledTransaction<EvTxEnvelope>,
}

impl EvPooledTransaction {
    pub fn new(transaction: Recovered<EvTxEnvelope>, encoded_length: usize) -> Self {
        Self { inner: EthPooledTransaction::new(transaction, encoded_length) }
    }

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
        self.inner.ty()
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
            std::mem::replace(&mut self.inner.blob_sidecar, EthBlobTransactionSidecar::Missing)
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
                let pooled_transaction =
                    tx.try_into_pooled_eip4844(std::sync::Arc::unwrap_or_clone(sidecar)).ok()?;
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
                None => Err(BlobTransactionValidationError::NotBlobTransaction(self.ty())),
            },
            EvTxEnvelope::EvNode(_) => Err(BlobTransactionValidationError::NotBlobTransaction(self.ty())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EvTxPoolError {
    #[error("evnode transaction must include at least one call")]
    EmptyCalls,
    #[error("only the first call may be CREATE")]
    InvalidCreatePosition,
    #[error("invalid sponsor signature")]
    InvalidSponsorSignature,
    #[error("state provider error: {0}")]
    StateProvider(String),
}

impl PoolTransactionError for EvTxPoolError {
    fn is_bad_transaction(&self) -> bool {
        matches!(
            self,
            Self::EmptyCalls | Self::InvalidCreatePosition | Self::InvalidSponsorSignature
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone)]
pub struct EvTransactionValidator<Client> {
    inner: Arc<EthTransactionValidator<Client, EvPooledTransaction>>,
}

impl<Client> EvTransactionValidator<Client> {
    pub fn new(inner: EthTransactionValidator<Client, EvPooledTransaction>) -> Self {
        Self { inner: Arc::new(inner) }
    }

    fn validate_evnode_calls(&self, tx: &EvNodeTransaction) -> Result<(), InvalidPoolTransactionError> {
        if tx.calls.is_empty() {
            return Err(InvalidPoolTransactionError::other(EvTxPoolError::EmptyCalls));
        }
        if tx.calls.iter().skip(1).any(|call| call.to.is_create()) {
            return Err(InvalidPoolTransactionError::other(EvTxPoolError::InvalidCreatePosition));
        }
        Ok(())
    }

    fn ensure_state(
        &self,
        state: &mut Option<Box<dyn AccountInfoReader>>,
    ) -> Result<(), InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        if state.is_none() {
            let new_state = self
                .inner
                .client()
                .latest()
                .map_err(|err| InvalidPoolTransactionError::other(EvTxPoolError::StateProvider(err.to_string())))?;
            *state = Some(Box::new(new_state));
        }
        Ok(())
    }

    fn validate_sponsor_balance(
        &self,
        state: &mut Option<Box<dyn AccountInfoReader>>,
        sponsor: Address,
        gas_cost: U256,
    ) -> Result<(), InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        self.ensure_state(state)?;
        let state = state.as_ref().expect("state provider is set");
        let account = state
            .basic_account(&sponsor)
            .map_err(|err| InvalidPoolTransactionError::other(EvTxPoolError::StateProvider(err.to_string())))?
            .unwrap_or_default();
        if account.balance < gas_cost {
            return Err(InvalidPoolTransactionError::Overdraft { cost: gas_cost, balance: account.balance });
        }
        Ok(())
    }

    fn validate_evnode(
        &self,
        pooled: &EvPooledTransaction,
        sender_balance: U256,
        state: &mut Option<Box<dyn AccountInfoReader>>,
    ) -> Result<(), InvalidPoolTransactionError>
    where
        Client: StateProviderFactory,
    {
        let consensus = pooled.transaction().inner();
        let EvTxEnvelope::EvNode(tx) = consensus else {
            if sender_balance < *pooled.cost() {
                return Err(InvalidPoolTransactionError::Overdraft { cost: *pooled.cost(), balance: sender_balance });
            }
            return Ok(());
        };

        let tx = tx.tx();
        self.validate_evnode_calls(tx)?;

        if let Some(signature) = tx.fee_payer_signature.as_ref() {
            let executor = pooled.transaction().signer();
            let sponsor = tx
                .recover_sponsor(executor, signature)
                .map_err(|_| InvalidPoolTransactionError::other(EvTxPoolError::InvalidSponsorSignature))?;

            let gas_cost = U256::from(tx.max_fee_per_gas).saturating_mul(U256::from(tx.gas_limit));
            self.validate_sponsor_balance(state, sponsor, gas_cost)?;
        }

        Ok(())
    }
}

impl<Client> TransactionValidator for EvTransactionValidator<Client>
where
    Client: ChainSpecProvider<ChainSpec: EthereumHardforks> + StateProviderFactory,
{
    type Transaction = EvPooledTransaction;

    async fn validate_transaction(
        &self,
        origin: TransactionOrigin,
        transaction: <Self as TransactionValidator>::Transaction,
    ) -> TransactionValidationOutcome<Self::Transaction> {
        let mut state = None;
        let outcome = self
            .inner
            .validate_one_with_state(origin, transaction, &mut state);

        match outcome {
            TransactionValidationOutcome::Valid {
                balance,
                state_nonce,
                bytecode_hash,
                transaction,
                propagate,
                authorities,
            } => match self.validate_evnode(transaction.transaction(), balance, &mut state) {
                Ok(()) => TransactionValidationOutcome::Valid {
                    balance,
                    state_nonce,
                    bytecode_hash,
                    transaction,
                    propagate,
                    authorities,
                },
                Err(err) => TransactionValidationOutcome::Invalid(transaction.into_transaction(), err),
            },
            other => other,
        }
    }
}

/// Pool builder that wires the custom EvNode transaction validator.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EvolvePoolBuilder;

impl<Types, Node> PoolBuilder<Node> for EvolvePoolBuilder
where
    Types: NodeTypes<
        ChainSpec: EthereumHardforks,
        Primitives: NodePrimitives<SignedTx = TransactionSigned>,
    >,
    Node: FullNodeTypes<Types = Types>,
{
    type Pool = reth_transaction_pool::Pool<
        TransactionValidationTaskExecutor<EvTransactionValidator<Node::Provider>>,
        CoinbaseTipOrdering<EvPooledTransaction>,
        DiskFileBlobStore,
    >;

    async fn build_pool(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Pool> {
        let pool_config = ctx.pool_config();

        let blobs_disabled = ctx.config().txpool.blobpool_max_count == 0;

        let blob_cache_size = if let Some(blob_cache_size) = pool_config.blob_cache_size {
            Some(blob_cache_size)
        } else {
            let current_timestamp =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
            let blob_params = ctx
                .chain_spec()
                .blob_params_at_timestamp(current_timestamp)
                .unwrap_or_else(BlobParams::cancun);

            Some((blob_params.target_blob_count * EPOCH_SLOTS * 2) as u32)
        };

        let blob_store = create_blob_store_with_cache(ctx, blob_cache_size)?;

        let validator = TransactionValidationTaskExecutor::eth_builder(ctx.provider().clone())
            .with_head_timestamp(ctx.head().timestamp)
            .set_eip4844(!blobs_disabled)
            .kzg_settings(ctx.kzg_settings()?)
            .with_max_tx_input_bytes(ctx.config().txpool.max_tx_input_bytes)
            .with_local_transactions_config(pool_config.local_transactions_config.clone())
            .set_tx_fee_cap(ctx.config().rpc.rpc_tx_fee_cap)
            .with_max_tx_gas_limit(ctx.config().txpool.max_tx_gas_limit)
            .with_minimum_priority_fee(ctx.config().txpool.minimum_priority_fee)
            .disable_balance_check()
            .with_additional_tasks(ctx.config().txpool.additional_validation_tasks)
            .build_with_tasks::<EvPooledTransaction, _, _>(ctx.task_executor().clone(), blob_store.clone())
            .map(EvTransactionValidator::new);

        if validator.validator().inner.eip4844() {
            let kzg_settings = validator.validator().inner.kzg_settings().clone();
            ctx.task_executor().spawn_blocking(async move {
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
