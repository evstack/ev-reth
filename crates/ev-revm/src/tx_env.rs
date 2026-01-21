use alloy_evm::{FromRecoveredTx, FromTxWithEncoded};
use alloy_primitives::{Address, Bytes, U256};
use ev_primitives::{Call, EvTxEnvelope};
use reth_evm::TransactionEnv;
use reth_revm::revm::{
    context::TxEnv,
    context_interface::{
        either::Either,
        transaction::{
            AccessList, AccessListItem, RecoveredAuthorization, SignedAuthorization,
            Transaction as RevmTransaction, TransactionType,
        },
    },
    handler::SystemCallTx,
    primitives::{Address as RevmAddress, Bytes as RevmBytes, TxKind, B256},
};

/// Transaction environment wrapper that supports `EvTxEnvelope` conversions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvTxEnv {
    inner: TxEnv,
    sponsor: Option<Address>,
    sponsor_signature_invalid: bool,
    calls: Vec<Call>,
    batch_value: U256,
}

impl EvTxEnv {
    /// Wrap a `TxEnv` with EV-specific metadata.
    pub const fn new(inner: TxEnv) -> Self {
        Self {
            batch_value: inner.value,
            inner,
            sponsor: None,
            sponsor_signature_invalid: false,
            calls: Vec::new(),
        }
    }

    /// Returns the underlying `TxEnv`.
    pub const fn inner(&self) -> &TxEnv {
        &self.inner
    }

    /// Returns the underlying `TxEnv` mutably.
    pub fn inner_mut(&mut self) -> &mut TxEnv {
        &mut self.inner
    }

    /// Returns the recovered sponsor address, if any.
    pub const fn sponsor(&self) -> Option<Address> {
        self.sponsor
    }

    /// Returns whether the sponsor signature was invalid.
    pub const fn sponsor_signature_invalid(&self) -> bool {
        self.sponsor_signature_invalid
    }

    /// Returns the batch calls for this transaction.
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    /// Returns the total value across all calls.
    pub const fn batch_value(&self) -> U256 {
        self.batch_value
    }

    /// Updates the inner `TxEnv` to represent a single call from the batch.
    pub fn set_call(&mut self, call: &Call) {
        self.inner.kind = call.to;
        self.inner.value = call.value;
        self.inner.data = call.input.clone();
    }
}

#[cfg(test)]
impl EvTxEnv {
    /// Test helper to build an `EvTxEnv` with batch calls pre-populated.
    pub fn with_calls(mut inner: TxEnv, calls: Vec<Call>) -> Self {
        let batch_value = calls
            .iter()
            .fold(U256::ZERO, |acc, call| acc.saturating_add(call.value));
        if let Some(first) = calls.first() {
            inner.kind = first.to;
            inner.data = first.input.clone();
        }
        inner.value = batch_value;
        let mut env = Self::new(inner);
        env.calls = calls;
        env.batch_value = batch_value;
        env
    }
}

impl From<TxEnv> for EvTxEnv {
    fn from(inner: TxEnv) -> Self {
        Self {
            batch_value: inner.value,
            inner,
            sponsor: None,
            sponsor_signature_invalid: false,
            calls: Vec::new(),
        }
    }
}

impl From<EvTxEnv> for TxEnv {
    fn from(env: EvTxEnv) -> Self {
        env.inner
    }
}

impl RevmTransaction for EvTxEnv {
    type AccessListItem<'a>
        = &'a AccessListItem
    where
        Self: 'a;
    type Authorization<'a>
        = &'a Either<SignedAuthorization, RecoveredAuthorization>
    where
        Self: 'a;

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

impl alloy_evm::ToTxEnv<Self> for EvTxEnv {
    fn to_tx_env(&self) -> Self {
        self.clone()
    }
}

impl FromRecoveredTx<EvTxEnvelope> for EvTxEnv {
    fn from_recovered_tx(tx: &EvTxEnvelope, sender: Address) -> Self {
        match tx {
            EvTxEnvelope::Ethereum(inner) => Self::new(TxEnv::from_recovered_tx(inner, sender)),
            EvTxEnvelope::EvNode(ev) => {
                let (sponsor, sponsor_signature_invalid) =
                    if let Some(signature) = ev.tx().fee_payer_signature.as_ref() {
                        match ev.tx().recover_sponsor(sender, signature) {
                            Ok(sponsor) => (Some(sponsor), false),
                            Err(_) => (None, true),
                        }
                    } else {
                        (None, false)
                    };
                let calls = ev.tx().calls.clone();
                let batch_value = calls
                    .iter()
                    .fold(U256::ZERO, |acc, call| acc.saturating_add(call.value));
                let env = TxEnv {
                    caller: sender,
                    gas_limit: ev.tx().gas_limit,
                    gas_price: ev.tx().max_fee_per_gas,
                    gas_priority_fee: Some(ev.tx().max_priority_fee_per_gas),
                    kind: ev
                        .tx()
                        .calls
                        .first()
                        .map(|call| call.to)
                        .unwrap_or(TxKind::Create),
                    value: batch_value,
                    data: ev
                        .tx()
                        .calls
                        .first()
                        .map(|call| call.input.clone())
                        .unwrap_or_default(),
                    nonce: ev.tx().nonce,
                    chain_id: Some(ev.tx().chain_id),
                    access_list: ev.tx().access_list.clone(),
                    tx_type: TransactionType::Eip1559.into(),
                    ..Default::default()
                };
                Self {
                    inner: env,
                    sponsor,
                    sponsor_signature_invalid,
                    calls,
                    batch_value,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EvTxEnv;
    use alloy_evm::FromRecoveredTx;
    use alloy_primitives::{Address, Bytes, Signature, TxKind, U256};
    use ev_primitives::{Call, EvNodeSignedTx, EvNodeTransaction, EvTxEnvelope};

    fn sample_evnode_tx() -> EvNodeTransaction {
        EvNodeTransaction {
            chain_id: 1,
            nonce: 1,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: 2,
            gas_limit: 21_000,
            calls: vec![Call {
                to: TxKind::Call(Address::ZERO),
                value: U256::ZERO,
                input: Bytes::default(),
            }],
            access_list: Default::default(),
            fee_payer_signature: None,
        }
    }

    fn signature_with_parity(v: u8, r: u8, s: u8) -> Signature {
        let mut bytes = [0u8; 65];
        bytes[0] = r;
        bytes[32] = s;
        bytes[64] = v;
        Signature::from_raw_array(&bytes).expect("valid parity")
    }

    #[test]
    fn from_recovered_tx_marks_invalid_sponsor_signature() {
        let executor = Address::from([0x11; 20]);
        let mut tx = sample_evnode_tx();
        tx.fee_payer_signature = Some(signature_with_parity(27, 0, 0));

        let signed = EvNodeSignedTx::new_unhashed(tx, signature_with_parity(27, 1, 1));
        let env = EvTxEnv::from_recovered_tx(&EvTxEnvelope::EvNode(signed), executor);

        assert!(env.sponsor().is_none(), "invalid signature should not recover sponsor");
        assert!(
            env.sponsor_signature_invalid(),
            "invalid signature should be flagged"
        );
    }

    #[test]
    fn from_recovered_tx_allows_missing_sponsor_signature() {
        let executor = Address::from([0x22; 20]);
        let tx = sample_evnode_tx();

        let signed = EvNodeSignedTx::new_unhashed(tx, signature_with_parity(27, 1, 1));
        let env = EvTxEnv::from_recovered_tx(&EvTxEnvelope::EvNode(signed), executor);

        assert!(env.sponsor().is_none());
        assert!(!env.sponsor_signature_invalid());
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
        Self::new(
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

/// Exposes the optional sponsor payer for EV-specific transactions.
pub trait SponsorPayerTx {
    /// Returns the sponsor address, if any.
    fn sponsor(&self) -> Option<Address>;
    /// Returns whether the sponsor signature was invalid.
    fn sponsor_signature_invalid(&self) -> bool;
}

/// Batch-call helpers for EV transactions.
pub trait BatchCallsTx {
    /// Returns the batch calls, if present.
    fn batch_calls(&self) -> Option<&[Call]>;
    /// Returns the total value across all calls.
    fn batch_total_value(&self) -> U256;
    /// Sets the inner `TxEnv` to the given call.
    fn set_batch_call(&mut self, call: &Call);
}

impl SponsorPayerTx for EvTxEnv {
    fn sponsor(&self) -> Option<Address> {
        self.sponsor
    }

    fn sponsor_signature_invalid(&self) -> bool {
        self.sponsor_signature_invalid
    }
}

impl BatchCallsTx for EvTxEnv {
    fn batch_calls(&self) -> Option<&[Call]> {
        if self.calls.is_empty() {
            None
        } else {
            Some(&self.calls)
        }
    }

    fn batch_total_value(&self) -> U256 {
        self.batch_value
    }

    fn set_batch_call(&mut self, call: &Call) {
        self.set_call(call);
    }
}

impl SponsorPayerTx for TxEnv {
    fn sponsor(&self) -> Option<Address> {
        None
    }

    fn sponsor_signature_invalid(&self) -> bool {
        false
    }
}

impl BatchCallsTx for TxEnv {
    fn batch_calls(&self) -> Option<&[Call]> {
        None
    }

    fn batch_total_value(&self) -> U256 {
        self.value
    }

    fn set_batch_call(&mut self, call: &Call) {
        self.kind = call.to;
        self.value = call.value;
        self.data = call.input.clone();
    }
}
