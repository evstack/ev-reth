//! RPC wiring for EvTxEnvelope support.

use alloy_consensus::error::ValueError;
use alloy_consensus::transaction::Recovered;
use alloy_consensus::SignableTransaction;
use alloy_consensus_any::AnyReceiptEnvelope;
use alloy_network::{Ethereum, TxSigner};
use alloy_primitives::{Address, Signature};
use alloy_rpc_types_eth::{Log, Transaction, TransactionInfo, TransactionRequest, TransactionReceipt};
use reth_chainspec::{ChainSpecProvider, EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{ConfigureEvm, SpecFor, TxEnvFor};
use reth_node_api::{FullNodeComponents, FullNodeTypes, NodeTypes};
use reth_node_builder::rpc::{EthApiBuilder, EthApiCtx};
use reth_rpc::EthApi;
use reth_rpc_convert::transaction::{
    ConvertReceiptInput, EthTxEnvError, ReceiptConverter, RpcTxConverter, SimTxConverter,
    TryIntoSimTx, TryIntoTxEnv, TxEnvConverter,
};
use reth_rpc_convert::{
    RpcConvert, RpcConverter, RpcTransaction, RpcTxReq, RpcTypes, SignTxRequestError,
    SignableTxRequest,
};
use reth_rpc_eth_api::{
    helpers::{pending_block::BuildPendingEnv, AddDevSigners},
    FullEthApiServer, FromEvmError, RpcNodeCore,
};
use reth_rpc_eth_types::receipt::build_receipt;
use reth_rpc_eth_types::EthApiError;
use std::marker::PhantomData;

use ev_primitives::{EvPrimitives, EvTxEnvelope};
use ev_revm::EvTxEnv;
use crate::EvolveEvmConfig;

/// Ev-specific RPC types using Ethereum responses with a custom request wrapper.
#[derive(Clone, Debug)]
pub struct EvRpcTypes;

impl RpcTypes for EvRpcTypes {
    type Header = <Ethereum as RpcTypes>::Header;
    type Receipt = TransactionReceipt<AnyReceiptEnvelope<Log>>;
    type TransactionResponse = <Ethereum as RpcTypes>::TransactionResponse;
    type TransactionRequest = EvTransactionRequest;
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
        let mut tx =
            self.0.build_typed_tx().map_err(|_| SignTxRequestError::InvalidTransactionRequest)?;
        let signature = signer.sign_transaction(&mut tx).await?;
        let signed: reth_ethereum_primitives::TransactionSigned =
            tx.into_signed(signature).into();
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
        cfg_env: &reth_revm::revm::context::CfgEnv<Spec>,
        block_env: &reth_revm::revm::context::BlockEnv,
    ) -> Result<EvTxEnv, Self::Err> {
        self.0
            .try_into_tx_env(cfg_env, block_env)
            .map(EvTxEnv::from)
    }
}

/// Receipt converter for EvPrimitives.
#[derive(Debug, Clone)]
pub struct EvReceiptConverter<ChainSpec> {
    chain_spec: std::sync::Arc<ChainSpec>,
}

impl<ChainSpec> EvReceiptConverter<ChainSpec> {
    pub const fn new(chain_spec: std::sync::Arc<ChainSpec>) -> Self {
        Self { chain_spec }
    }
}

impl<ChainSpec> ReceiptConverter<EvPrimitives> for EvReceiptConverter<ChainSpec>
where
    ChainSpec: EthChainSpec + 'static,
{
    type RpcReceipt = TransactionReceipt<AnyReceiptEnvelope<Log>>;
    type Error = EthApiError;

    fn convert_receipts(
        &self,
        inputs: Vec<ConvertReceiptInput<'_, EvPrimitives>>,
    ) -> Result<Vec<Self::RpcReceipt>, Self::Error> {
        let mut receipts = Vec::with_capacity(inputs.len());

        for input in inputs {
            let blob_params = self.chain_spec.blob_params_at_timestamp(input.meta.timestamp);
            receipts.push(build_receipt(input, blob_params, |receipt, next_log_index, meta| {
                let rpc_receipt = receipt.into_rpc(next_log_index, meta);
                let tx_type = u8::from(rpc_receipt.tx_type);
                let inner = <alloy_consensus::Receipt<Log>>::from(rpc_receipt).with_bloom();
                AnyReceiptEnvelope { inner, r#type: tx_type }
            }));
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

/// Builds [`EthApi`] for EvTxEnvelope nodes.
#[derive(Debug, Default)]
pub struct EvEthApiBuilder;

impl<N> EthApiBuilder<N> for EvEthApiBuilder
where
    N: FullNodeComponents<
        Types: NodeTypes<
            Primitives = EvPrimitives,
            ChainSpec:
                Hardforks + EthereumHardforks + EthChainSpec + std::fmt::Debug + Send + Sync + 'static,
        >,
        Evm = EvolveEvmConfig,
    > + RpcNodeCore<
        Primitives = EvPrimitives,
        Provider = <N as FullNodeTypes>::Provider,
        Pool = <N as FullNodeComponents>::Pool,
        Evm = EvolveEvmConfig,
    >,
    <N as FullNodeTypes>::Provider: ChainSpecProvider<
        ChainSpec = <<N as FullNodeTypes>::Types as NodeTypes>::ChainSpec,
    >,
    <N as FullNodeComponents>::Evm:
        ConfigureEvm<NextBlockEnvCtx: BuildPendingEnv<alloy_consensus::Header>>,
    TxEnvFor<<N as FullNodeComponents>::Evm>: From<EvTxEnv>,
    EvRpcConvert<N>: RpcConvert<
        Primitives = EvPrimitives,
        TxEnv = TxEnvFor<<N as FullNodeComponents>::Evm>,
        Error = EthApiError,
        Network = EvRpcTypes,
        Spec = SpecFor<<N as FullNodeComponents>::Evm>,
    >,
    EthApiError: FromEvmError<<N as FullNodeComponents>::Evm>,
    EvEthApiFor<N>: FullEthApiServer<
            Provider = <N as FullNodeTypes>::Provider,
            Pool = <N as FullNodeComponents>::Pool,
        > + AddDevSigners,
{
    type EthApi = EvEthApiFor<N>;

    async fn build_eth_api(self, ctx: EthApiCtx<'_, N>) -> eyre::Result<Self::EthApi> {
        let receipt_converter =
            EvReceiptConverter::new(FullNodeComponents::provider(ctx.components).chain_spec());
        let rpc_converter = RpcConverter::new(receipt_converter)
            .with_sim_tx_converter(EvSimTxConverter::default())
            .with_rpc_tx_converter(EvRpcTxConverter::default());
        let rpc_converter =
            rpc_converter.with_tx_env_converter(EvTxEnvConverter::<EvolveEvmConfig>::default());

        Ok(ctx.eth_api_builder().with_rpc_converter(rpc_converter).build())
    }
}

/// Converts EvTxEnvelope into RPC transaction responses.
#[derive(Clone, Debug, Default)]
pub struct EvRpcTxConverter;

impl RpcTxConverter<EvTxEnvelope, RpcTransaction<EvRpcTypes>, TransactionInfo> for EvRpcTxConverter {
    type Err = EthApiError;

    fn convert_rpc_tx(
        &self,
        tx: EvTxEnvelope,
        signer: Address,
        tx_info: TransactionInfo,
    ) -> Result<RpcTransaction<EvRpcTypes>, Self::Err> {
        match tx {
            EvTxEnvelope::Ethereum(inner) => Ok(Transaction::from_transaction(
                Recovered::new_unchecked(inner.into(), signer),
                tx_info,
            )),
            EvTxEnvelope::EvNode(_) => Err(EthApiError::TransactionConversionError),
        }
    }
}

/// Converts transaction requests into simulated EvTxEnvelope transactions.
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

/// Converts transaction requests into EvTxEnv.
#[derive(Clone, Debug)]
pub struct EvTxEnvConverter<Evm>(PhantomData<Evm>);

impl<Evm> Default for EvTxEnvConverter<Evm> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Evm> TxEnvConverter<RpcTxReq<EvRpcTypes>, TxEnvFor<Evm>, SpecFor<Evm>>
    for EvTxEnvConverter<Evm>
where
    Evm: ConfigureEvm + Send + Sync + 'static,
    TxEnvFor<Evm>: From<EvTxEnv>,
{
    type Error = EthTxEnvError;

    fn convert_tx_env(
        &self,
        tx_req: RpcTxReq<EvRpcTypes>,
        cfg_env: &reth_revm::revm::context::CfgEnv<SpecFor<Evm>>,
        block_env: &reth_revm::revm::context::BlockEnv,
    ) -> Result<TxEnvFor<Evm>, Self::Error> {
        tx_req
            .0
            .try_into_tx_env(cfg_env, block_env)
            .map(EvTxEnv::from)
            .map(Into::into)
    }
}
