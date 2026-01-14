use alloy_evm::{FromRecoveredTx, FromTxWithEncoded};
use alloy_primitives::{Address, Bytes};
use ev_primitives::EvTxEnvelope;
use reth_revm::revm::context::TxEnv;
use reth_revm::revm::context_interface::transaction::{
    AccessList, AccessListItem, RecoveredAuthorization, SignedAuthorization,
    Transaction as RevmTransaction,
};
use reth_revm::revm::handler::SystemCallTx;
use reth_revm::revm::primitives::{Address as RevmAddress, Bytes as RevmBytes, TxKind, B256, U256};
use reth_revm::revm::context_interface::either::Either;
use reth_evm::TransactionEnv;

/// Transaction environment wrapper that supports EvTxEnvelope conversions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvTxEnv {
    inner: TxEnv,
}

impl EvTxEnv {
    pub const fn new(inner: TxEnv) -> Self {
        Self { inner }
    }

    pub const fn inner(&self) -> &TxEnv {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut TxEnv {
        &mut self.inner
    }
}

impl From<TxEnv> for EvTxEnv {
    fn from(inner: TxEnv) -> Self {
        Self { inner }
    }
}

impl From<EvTxEnv> for TxEnv {
    fn from(env: EvTxEnv) -> Self {
        env.inner
    }
}

impl RevmTransaction for EvTxEnv {
    type AccessListItem<'a> = &'a AccessListItem where Self: 'a;
    type Authorization<'a> = &'a Either<SignedAuthorization, RecoveredAuthorization> where Self: 'a;

    fn tx_type(&self) -> u8 {
        self.inner.tx_type
    }

    fn caller(&self) -> RevmAddress {
        self.inner.caller
    }

    fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }

    fn value(&self) -> U256 {
        self.inner.value
    }

    fn input(&self) -> &RevmBytes {
        &self.inner.data
    }

    fn nonce(&self) -> u64 {
        self.inner.nonce
    }

    fn kind(&self) -> TxKind {
        self.inner.kind
    }

    fn chain_id(&self) -> Option<u64> {
        self.inner.chain_id
    }

    fn gas_price(&self) -> u128 {
        self.inner.gas_price
    }

    fn access_list(&self) -> Option<impl Iterator<Item = Self::AccessListItem<'_>>> {
        Some(self.inner.access_list.0.iter())
    }

    fn blob_versioned_hashes(&self) -> &[B256] {
        &self.inner.blob_hashes
    }

    fn max_fee_per_blob_gas(&self) -> u128 {
        self.inner.max_fee_per_blob_gas
    }

    fn authorization_list_len(&self) -> usize {
        self.inner.authorization_list.len()
    }

    fn authorization_list(&self) -> impl Iterator<Item = Self::Authorization<'_>> {
        self.inner.authorization_list.iter()
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        self.inner.gas_priority_fee
    }
}

impl TransactionEnv for EvTxEnv {
    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.inner.gas_limit = gas_limit;
    }

    fn nonce(&self) -> u64 {
        self.inner.nonce
    }

    fn set_nonce(&mut self, nonce: u64) {
        self.inner.nonce = nonce;
    }

    fn set_access_list(&mut self, access_list: AccessList) {
        self.inner.access_list = access_list;
    }
}

impl alloy_evm::ToTxEnv<EvTxEnv> for EvTxEnv {
    fn to_tx_env(&self) -> EvTxEnv {
        self.clone()
    }
}

impl FromRecoveredTx<EvTxEnvelope> for EvTxEnv {
    fn from_recovered_tx(tx: &EvTxEnvelope, sender: Address) -> Self {
        match tx {
            EvTxEnvelope::Ethereum(inner) => EvTxEnv::new(TxEnv::from_recovered_tx(inner, sender)),
            EvTxEnvelope::EvNode(ev) => {
                let mut env = TxEnv::default();
                env.caller = sender;
                env.gas_limit = ev.tx().gas_limit;
                env.gas_price = ev.tx().max_fee_per_gas;
                env.kind = ev.tx().calls.first().map(|call| call.to).unwrap_or(TxKind::Create);
                env.value = ev.tx().calls.first().map(|call| call.value).unwrap_or_default();
                env.data = ev.tx().calls.first().map(|call| call.input.clone()).unwrap_or_default();
                EvTxEnv::new(env)
            }
        }
    }
}

impl FromTxWithEncoded<EvTxEnvelope> for EvTxEnv {
    fn from_encoded_tx(tx: &EvTxEnvelope, caller: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, caller)
    }
}

impl SystemCallTx for EvTxEnv {
    fn new_system_tx_with_caller(
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Self {
        EvTxEnv::new(
            TxEnv::builder()
                .caller(caller)
                .data(data)
                .kind(TxKind::Call(system_contract_address))
                .gas_limit(30_000_000)
                .build()
                .unwrap(),
        )
    }
}
