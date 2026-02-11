//! RPC wiring for EvTxEnvelope support.

use alloy_consensus::{
    error::ValueError, transaction::Recovered, SignableTransaction,
    Transaction as ConsensusTransaction,
};
use alloy_consensus_any::AnyReceiptEnvelope;
use alloy_network::{Ethereum, ReceiptResponse, TransactionResponse, TxSigner};
use alloy_primitives::{Address, Signature, U256};
use alloy_rpc_types_eth::{
    Log, Transaction, TransactionInfo, TransactionReceipt, TransactionRequest,
};
use reth_chainspec::{ChainSpecProvider, EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{ConfigureEvm, EvmEnvFor, TxEnvFor};
use reth_node_api::{FullNodeComponents, FullNodeTypes, NodeTypes};
use reth_node_builder::rpc::{EthApiBuilder, EthApiCtx};
use reth_rpc::EthApi;
use reth_rpc_convert::{
    transaction::{
        ConvertReceiptInput, ReceiptConverter, RpcTxConverter, SimTxConverter, TryIntoSimTx,
        TxEnvConverter,
    },
    EthTxEnvError, RpcConvert, RpcConverter, RpcTransaction, RpcTxReq, RpcTypes,
    SignTxRequestError, SignableTxRequest, TryIntoTxEnv,
};
use reth_rpc_eth_api::{
    helpers::pending_block::BuildPendingEnv, FromEvmError, FullEthApiServer, RpcNodeCore,
};
use reth_rpc_eth_types::{receipt::build_receipt, EthApiError};
use std::marker::PhantomData;

use crate::EvolveEvmConfig;
use ev_primitives::{EvPrimitives, EvTxEnvelope};
use ev_revm::EvTxEnv;

/// Ev-specific RPC types using Ethereum responses with a custom request wrapper.
#[derive(Clone, Debug)]
pub struct EvRpcTypes;

impl RpcTypes for EvRpcTypes {
    type Header = <Ethereum as RpcTypes>::Header;
    type Receipt = EvRpcReceipt;
    type TransactionResponse = EvRpcTransaction;
    type TransactionRequest = EvTransactionRequest;
}

/// RPC transaction response with optional sponsor address.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EvRpcTransaction {
    #[serde(flatten)]
    inner: Transaction<EvTxEnvelope>,
    #[serde(rename = "feePayer", skip_serializing_if = "Option::is_none")]
    fee_payer: Option<Address>,
}

impl EvRpcTransaction {
    const fn new(inner: Transaction<EvTxEnvelope>, fee_payer: Option<Address>) -> Self {
        Self { inner, fee_payer }
    }

    /// Returns the optional fee payer address.
    pub const fn fee_payer(&self) -> Option<Address> {
        self.fee_payer
    }

    /// Returns the inner transaction.
    pub const fn inner(&self) -> &Transaction<EvTxEnvelope> {
        &self.inner
    }
}

impl ConsensusTransaction for EvRpcTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        ConsensusTransaction::chain_id(&self.inner)
    }

    fn nonce(&self) -> u64 {
        ConsensusTransaction::nonce(&self.inner)
    }

    fn gas_limit(&self) -> u64 {
        ConsensusTransaction::gas_limit(&self.inner)
    }

    fn gas_price(&self) -> Option<u128> {
        ConsensusTransaction::gas_price(&self.inner)
    }

    fn max_fee_per_gas(&self) -> u128 {
        ConsensusTransaction::max_fee_per_gas(&self.inner)
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        ConsensusTransaction::max_priority_fee_per_gas(&self.inner)
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        ConsensusTransaction::max_fee_per_blob_gas(&self.inner)
    }

    fn priority_fee_or_price(&self) -> u128 {
        ConsensusTransaction::priority_fee_or_price(&self.inner)
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        ConsensusTransaction::effective_gas_price(&self.inner, base_fee)
    }

    fn is_dynamic_fee(&self) -> bool {
        ConsensusTransaction::is_dynamic_fee(&self.inner)
    }

    fn kind(&self) -> alloy_primitives::TxKind {
        ConsensusTransaction::kind(&self.inner)
    }

    fn is_create(&self) -> bool {
        ConsensusTransaction::is_create(&self.inner)
    }

    fn value(&self) -> U256 {
        ConsensusTransaction::value(&self.inner)
    }

    fn input(&self) -> &alloy_primitives::Bytes {
        ConsensusTransaction::input(&self.inner)
    }

    fn access_list(&self) -> Option<&alloy_eips::eip2930::AccessList> {
        ConsensusTransaction::access_list(&self.inner)
    }

    fn blob_versioned_hashes(&self) -> Option<&[alloy_primitives::B256]> {
        ConsensusTransaction::blob_versioned_hashes(&self.inner)
    }

    fn authorization_list(&self) -> Option<&[alloy_eips::eip7702::SignedAuthorization]> {
        ConsensusTransaction::authorization_list(&self.inner)
    }
}

impl TransactionResponse for EvRpcTransaction {
    fn tx_hash(&self) -> alloy_primitives::TxHash {
        self.inner.tx_hash()
    }

    fn block_hash(&self) -> Option<alloy_primitives::BlockHash> {
        self.inner.block_hash()
    }

    fn block_number(&self) -> Option<u64> {
        self.inner.block_number()
    }

    fn transaction_index(&self) -> Option<u64> {
        self.inner.transaction_index()
    }

    fn from(&self) -> Address {
        self.inner.from()
    }
}

impl alloy_eips::Typed2718 for EvRpcTransaction {
    fn ty(&self) -> u8 {
        self.inner.ty()
    }
}

/// RPC receipt response with optional sponsor address.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EvRpcReceipt {
    #[serde(flatten)]
    inner: TransactionReceipt<AnyReceiptEnvelope<Log>>,
    #[serde(rename = "feePayer", skip_serializing_if = "Option::is_none")]
    fee_payer: Option<Address>,
}

impl EvRpcReceipt {
    const fn new(
        inner: TransactionReceipt<AnyReceiptEnvelope<Log>>,
        fee_payer: Option<Address>,
    ) -> Self {
        Self { inner, fee_payer }
    }

    /// Returns the optional fee payer address.
    pub const fn fee_payer(&self) -> Option<Address> {
        self.fee_payer
    }

    /// Returns the inner receipt.
    pub const fn inner(&self) -> &TransactionReceipt<AnyReceiptEnvelope<Log>> {
        &self.inner
    }
}

impl ReceiptResponse for EvRpcReceipt {
    fn contract_address(&self) -> Option<Address> {
        self.inner.contract_address()
    }

    fn status(&self) -> bool {
        self.inner.status()
    }

    fn block_hash(&self) -> Option<alloy_primitives::BlockHash> {
        self.inner.block_hash()
    }

    fn block_number(&self) -> Option<u64> {
        self.inner.block_number()
    }

    fn transaction_hash(&self) -> alloy_primitives::TxHash {
        self.inner.transaction_hash()
    }

    fn transaction_index(&self) -> Option<u64> {
        self.inner.transaction_index()
    }

    fn gas_used(&self) -> u64 {
        self.inner.gas_used()
    }

    fn effective_gas_price(&self) -> u128 {
        self.inner.effective_gas_price()
    }

    fn blob_gas_used(&self) -> Option<u64> {
        self.inner.blob_gas_used()
    }

    fn blob_gas_price(&self) -> Option<u128> {
        self.inner.blob_gas_price()
    }

    fn from(&self) -> Address {
        self.inner.from()
    }

    fn to(&self) -> Option<Address> {
        self.inner.to()
    }

    fn cumulative_gas_used(&self) -> u64 {
        self.inner.cumulative_gas_used()
    }

    fn state_root(&self) -> Option<alloy_primitives::B256> {
        self.inner.state_root()
    }
}

/// Transaction request wrapper to satisfy local trait bounds.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EvTransactionRequest(pub TransactionRequest);

impl From<TransactionRequest> for EvTransactionRequest {
    fn from(value: TransactionRequest) -> Self {
        Self(value)
    }
}

impl AsRef<TransactionRequest> for EvTransactionRequest {
    fn as_ref(&self) -> &TransactionRequest {
        &self.0
    }
}

impl AsMut<TransactionRequest> for EvTransactionRequest {
    fn as_mut(&mut self) -> &mut TransactionRequest {
        &mut self.0
    }
}

impl SignableTxRequest<EvTxEnvelope> for EvTransactionRequest {
    async fn try_build_and_sign(
        self,
        signer: impl TxSigner<Signature> + Send,
    ) -> Result<EvTxEnvelope, SignTxRequestError> {
        let mut tx = self
            .0
            .build_typed_tx()
            .map_err(|_| SignTxRequestError::InvalidTransactionRequest)?;
        let signature = signer.sign_transaction(&mut tx).await?;
        let signed: reth_ethereum_primitives::TransactionSigned = tx.into_signed(signature).into();
        Ok(EvTxEnvelope::Ethereum(signed))
    }
}

impl TryIntoSimTx<EvTxEnvelope> for EvTransactionRequest {
    fn try_into_sim_tx(self) -> Result<EvTxEnvelope, ValueError<Self>> {
        self.0
            .try_into_sim_tx()
            .map(EvTxEnvelope::Ethereum)
            .map_err(|err| err.map(EvTransactionRequest))
    }
}

impl TryIntoTxEnv<EvTxEnv> for EvTransactionRequest {
    type Err = EthTxEnvError;

    fn try_into_tx_env<Spec>(
        self,
        evm_env: &alloy_evm::EvmEnv<Spec>,
    ) -> Result<EvTxEnv, EthTxEnvError> {
        self.0.try_into_tx_env(evm_env).map(EvTxEnv::from)
    }
}

/// Receipt converter for `EvPrimitives`.
#[derive(Debug, Clone)]
pub struct EvReceiptConverter<ChainSpec> {
    chain_spec: std::sync::Arc<ChainSpec>,
}

impl<ChainSpec> EvReceiptConverter<ChainSpec> {
    /// Creates a new receipt converter bound to the provided chain spec.
    pub const fn new(chain_spec: std::sync::Arc<ChainSpec>) -> Self {
        Self { chain_spec }
    }
}

impl<ChainSpec> ReceiptConverter<EvPrimitives> for EvReceiptConverter<ChainSpec>
where
    ChainSpec: EthChainSpec + 'static,
{
    type RpcReceipt = EvRpcReceipt;
    type Error = EthApiError;

    fn convert_receipts(
        &self,
        inputs: Vec<ConvertReceiptInput<'_, EvPrimitives>>,
    ) -> Result<Vec<Self::RpcReceipt>, Self::Error> {
        let mut receipts = Vec::with_capacity(inputs.len());

        for input in inputs {
            let blob_params = self
                .chain_spec
                .blob_params_at_timestamp(input.meta.timestamp);
            let fee_payer = match input.tx.inner() {
                EvTxEnvelope::EvNode(ev) => ev
                    .tx()
                    .fee_payer_signature
                    .as_ref()
                    .and_then(|sig| ev.tx().recover_sponsor(input.tx.signer(), sig).ok()),
                EvTxEnvelope::Ethereum(_) => None,
            };
            let receipt = build_receipt(input, blob_params, |receipt, next_log_index, meta| {
                let rpc_receipt = receipt.into_rpc(next_log_index, meta);
                let tx_type = u8::from(rpc_receipt.tx_type);
                let inner = <alloy_consensus::Receipt<Log>>::from(rpc_receipt).with_bloom();
                AnyReceiptEnvelope {
                    inner,
                    r#type: tx_type,
                }
            });
            receipts.push(EvRpcReceipt::new(receipt, fee_payer));
        }

        Ok(receipts)
    }
}

/// RPC converter type for EvTxEnvelope-based nodes.
pub type EvRpcConvert<N> = RpcConverter<
    EvRpcTypes,
    EvolveEvmConfig,
    EvReceiptConverter<<<N as FullNodeTypes>::Types as NodeTypes>::ChainSpec>,
    (),
    (),
    EvSimTxConverter,
    EvRpcTxConverter,
    EvTxEnvConverter<EvolveEvmConfig>,
>;

/// Eth API type for EvTxEnvelope-based nodes.
pub type EvEthApiFor<N> = EthApi<N, EvRpcConvert<N>>;

/// Builds [`EthApi`] for `EvTxEnvelope` nodes.
#[derive(Debug, Default)]
pub struct EvEthApiBuilder;

impl<N> EthApiBuilder<N> for EvEthApiBuilder
where
    N: FullNodeComponents<
            Types: NodeTypes<
                Primitives = EvPrimitives,
                ChainSpec: Hardforks
                               + EthereumHardforks
                               + EthChainSpec
                               + std::fmt::Debug
                               + Send
                               + Sync
                               + 'static,
            >,
            Evm = EvolveEvmConfig,
        > + RpcNodeCore<
            Primitives = EvPrimitives,
            Provider = <N as FullNodeTypes>::Provider,
            Pool = <N as FullNodeComponents>::Pool,
            Evm = EvolveEvmConfig,
        >,
    <N as FullNodeTypes>::Provider:
        ChainSpecProvider<ChainSpec = <<N as FullNodeTypes>::Types as NodeTypes>::ChainSpec>,
    <N as FullNodeComponents>::Evm:
        ConfigureEvm<NextBlockEnvCtx: BuildPendingEnv<alloy_consensus::Header>>,
    TxEnvFor<<N as FullNodeComponents>::Evm>: From<EvTxEnv>,
    EvRpcConvert<N>: RpcConvert<
        Primitives = EvPrimitives,
        Evm = EvolveEvmConfig,
        Error = EthApiError,
        Network = EvRpcTypes,
    >,
    EthApiError: FromEvmError<<N as FullNodeComponents>::Evm>,
    EvEthApiFor<N>: FullEthApiServer<
        Provider = <N as FullNodeTypes>::Provider,
        Pool = <N as FullNodeComponents>::Pool,
    >,
{
    type EthApi = EvEthApiFor<N>;

    async fn build_eth_api(self, ctx: EthApiCtx<'_, N>) -> eyre::Result<Self::EthApi> {
        let receipt_converter =
            EvReceiptConverter::new(FullNodeComponents::provider(ctx.components).chain_spec());
        let rpc_converter = RpcConverter::new(receipt_converter)
            .with_sim_tx_converter(EvSimTxConverter)
            .with_rpc_tx_converter(EvRpcTxConverter);
        let rpc_converter =
            rpc_converter.with_tx_env_converter(EvTxEnvConverter::<EvolveEvmConfig>::default());

        Ok(ctx
            .eth_api_builder()
            .with_rpc_converter(rpc_converter)
            .build())
    }
}

/// Converts `EvTxEnvelope` into RPC transaction responses.
#[derive(Clone, Debug, Default)]
pub struct EvRpcTxConverter;

impl RpcTxConverter<EvTxEnvelope, RpcTransaction<EvRpcTypes>, TransactionInfo>
    for EvRpcTxConverter
{
    type Err = EthApiError;

    fn convert_rpc_tx(
        &self,
        tx: EvTxEnvelope,
        signer: Address,
        tx_info: TransactionInfo,
    ) -> Result<RpcTransaction<EvRpcTypes>, Self::Err> {
        let fee_payer = match &tx {
            EvTxEnvelope::EvNode(ev) => ev
                .tx()
                .fee_payer_signature
                .as_ref()
                .and_then(|sig| ev.tx().recover_sponsor(signer, sig).ok()),
            EvTxEnvelope::Ethereum(_) => None,
        };
        let recovered = Recovered::new_unchecked(tx, signer);
        Ok(EvRpcTransaction::new(
            Transaction::from_transaction(recovered, tx_info),
            fee_payer,
        ))
    }
}

/// Converts transaction requests into simulated `EvTxEnvelope` transactions.
#[derive(Clone, Debug, Default)]
pub struct EvSimTxConverter;

impl SimTxConverter<RpcTxReq<EvRpcTypes>, EvTxEnvelope> for EvSimTxConverter {
    type Err = ValueError<RpcTxReq<EvRpcTypes>>;

    fn convert_sim_tx(&self, tx_req: RpcTxReq<EvRpcTypes>) -> Result<EvTxEnvelope, Self::Err> {
        tx_req
            .0
            .try_into_sim_tx()
            .map(EvTxEnvelope::Ethereum)
            .map_err(|err| err.map(EvTransactionRequest))
    }
}

/// Converts transaction requests into `EvTxEnv`.
#[derive(Clone, Debug)]
pub struct EvTxEnvConverter<Evm>(PhantomData<Evm>);

impl<Evm> Default for EvTxEnvConverter<Evm> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Evm> TxEnvConverter<RpcTxReq<EvRpcTypes>, Evm> for EvTxEnvConverter<Evm>
where
    Evm: ConfigureEvm + Send + Sync + 'static,
    TxEnvFor<Evm>: From<EvTxEnv>,
{
    type Error = EthTxEnvError;

    fn convert_tx_env(
        &self,
        tx_req: RpcTxReq<EvRpcTypes>,
        evm_env: &EvmEnvFor<Evm>,
    ) -> Result<TxEnvFor<Evm>, Self::Error> {
        tx_req
            .0
            .try_into_tx_env(evm_env)
            .map(EvTxEnv::from)
            .map(Into::into)
    }
}
