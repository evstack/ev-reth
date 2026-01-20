//! Transaction types for ev-reth.

use alloy_consensus::{
    transaction::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx, SignerRecoverable, TxHashRef},
    SignableTransaction, Transaction, TransactionEnvelope,
};
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{keccak256, Address, Bytes, Signature, TxKind, B256, U256};
use alloy_rlp::{bytes::Buf, BufMut, Decodable, Encodable, Header, RlpDecodable, RlpEncodable};
use reth_codecs::{
    alloy::transaction::{CompactEnvelope, Envelope, FromTxCompact, ToTxCompact},
    txtype::COMPACT_EXTENDED_IDENTIFIER_FLAG,
    Compact,
};
use reth_db_api::{
    table::{Compress, Decompress},
    DatabaseError,
};
use reth_primitives_traits::{InMemorySize, SignedTransaction};
use std::vec::Vec;

/// EIP-2718 transaction type for EvNode batch + sponsorship.
pub const EVNODE_TX_TYPE_ID: u8 = 0x76;
/// Signature domain for sponsor authorization.
pub const EVNODE_SPONSOR_DOMAIN: u8 = 0x78;

/// Single call entry in an EvNode transaction.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    RlpEncodable,
    RlpDecodable,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Call {
    /// Destination (CALL or CREATE).
    pub to: TxKind,
    /// ETH value.
    pub value: U256,
    /// Calldata.
    pub input: Bytes,
}

/// EvNode batch + sponsorship transaction payload.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EvNodeTransaction {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub calls: Vec<Call>,
    pub access_list: AccessList,
    pub fee_payer_signature: Option<Signature>,
}

/// Signed EvNode transaction (executor signature).
pub type EvNodeSignedTx = alloy_consensus::Signed<EvNodeTransaction>;

/// Envelope type that includes standard Ethereum transactions and EvNode transactions.
#[derive(Clone, Debug, TransactionEnvelope)]
#[envelope(tx_type_name = EvTxType)]
pub enum EvTxEnvelope {
    /// Standard Ethereum typed transaction envelope.
    #[envelope(flatten)]
    Ethereum(reth_ethereum_primitives::TransactionSigned),
    /// EvNode typed transaction.
    #[envelope(ty = 0x76)]
    EvNode(EvNodeSignedTx),
}

/// Signed transaction type alias for ev-reth.
pub type TransactionSigned = EvTxEnvelope;

impl EvNodeTransaction {
    /// Returns the executor signing hash (domain 0x76, empty sponsor fields).
    pub fn executor_signing_hash(&self) -> B256 {
        let payload = self.encoded_payload(None);
        let mut preimage = Vec::with_capacity(1 + payload.len());
        preimage.push(EVNODE_TX_TYPE_ID);
        preimage.extend_from_slice(&payload);
        keccak256(preimage)
    }

    /// Returns the sponsor signing hash (domain 0x78, executor address bound).
    pub fn sponsor_signing_hash(&self, executor: Address) -> B256 {
        let payload = self.encoded_payload_with_executor(executor);
        let mut preimage = Vec::with_capacity(1 + payload.len());
        preimage.push(EVNODE_SPONSOR_DOMAIN);
        preimage.extend_from_slice(&payload);
        keccak256(preimage)
    }

    /// Recovers the executor address from the provided signature.
    pub fn recover_executor(
        &self,
        signature: &Signature,
    ) -> Result<Address, alloy_primitives::SignatureError> {
        signature.recover_address_from_prehash(&self.executor_signing_hash())
    }

    /// Recovers the sponsor address from the provided signature and executor address.
    pub fn recover_sponsor(
        &self,
        executor: Address,
        signature: &Signature,
    ) -> Result<Address, alloy_primitives::SignatureError> {
        signature.recover_address_from_prehash(&self.sponsor_signing_hash(executor))
    }

    fn first_call(&self) -> Option<&Call> {
        self.calls.first()
    }

    fn encoded_payload(&self, fee_payer_signature: Option<&Signature>) -> Vec<u8> {
        let payload_len = self.payload_fields_length(fee_payer_signature);
        let mut out = Vec::with_capacity(
            Header {
                list: true,
                payload_length: payload_len,
            }
            .length_with_payload(),
        );
        Header {
            list: true,
            payload_length: payload_len,
        }
        .encode(&mut out);
        self.encode_payload_fields(&mut out, fee_payer_signature);
        out
    }

    fn encoded_payload_with_executor(&self, executor: Address) -> Vec<u8> {
        // Sponsor signatures must be computed over the unsigned sponsor field to avoid
        // self-referential hashing.
        let mut out = Vec::with_capacity(self.payload_fields_length(None) + 32);
        out.extend_from_slice(executor.as_slice());
        self.encode_payload_fields(&mut out, None);
        out
    }

    fn payload_fields_length(&self, fee_payer_signature: Option<&Signature>) -> usize {
        self.chain_id.length()
            + self.nonce.length()
            + self.max_priority_fee_per_gas.length()
            + self.max_fee_per_gas.length()
            + self.gas_limit.length()
            + self.calls.length()
            + self.access_list.length()
            + optional_signature_length(fee_payer_signature)
    }

    fn encode_payload_fields(&self, out: &mut dyn BufMut, fee_payer_signature: Option<&Signature>) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.max_priority_fee_per_gas.encode(out);
        self.max_fee_per_gas.encode(out);
        self.gas_limit.encode(out);
        self.calls.encode(out);
        self.access_list.encode(out);
        encode_optional_signature(out, fee_payer_signature);
    }
}

impl Transaction for EvNodeTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        Some(self.chain_id)
    }

    fn nonce(&self) -> u64 {
        self.nonce
    }

    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    fn gas_price(&self) -> Option<u128> {
        None
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.max_fee_per_gas
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        Some(self.max_priority_fee_per_gas)
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }

    fn priority_fee_or_price(&self) -> u128 {
        self.max_priority_fee_per_gas
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        let max_fee = self.max_fee_per_gas;
        let Some(base_fee) = base_fee else {
            return max_fee;
        };
        let base_fee = base_fee as u128;
        if max_fee < base_fee {
            return max_fee;
        }
        let priority_fee = self.max_priority_fee_per_gas;
        let max_priority_fee = max_fee.saturating_sub(base_fee);
        base_fee.saturating_add(priority_fee.min(max_priority_fee))
    }

    fn is_dynamic_fee(&self) -> bool {
        true
    }

    fn kind(&self) -> TxKind {
        self.first_call()
            .map(|call| call.to)
            .unwrap_or(TxKind::Create)
    }

    fn is_create(&self) -> bool {
        matches!(self.first_call().map(|call| call.to), Some(TxKind::Create))
    }

    fn value(&self) -> U256 {
        self.calls
            .iter()
            .fold(U256::ZERO, |acc, call| acc.saturating_add(call.value))
    }

    fn input(&self) -> &Bytes {
        static EMPTY: Bytes = Bytes::new();
        self.first_call().map(|call| &call.input).unwrap_or(&EMPTY)
    }

    fn access_list(&self) -> Option<&AccessList> {
        Some(&self.access_list)
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    fn authorization_list(&self) -> Option<&[alloy_eips::eip7702::SignedAuthorization]> {
        None
    }
}

impl alloy_eips::Typed2718 for EvNodeTransaction {
    fn ty(&self) -> u8 {
        EVNODE_TX_TYPE_ID
    }
}

impl SignableTransaction<Signature> for EvNodeTransaction {
    fn set_chain_id(&mut self, chain_id: alloy_primitives::ChainId) {
        self.chain_id = chain_id;
    }

    fn encode_for_signing(&self, out: &mut dyn BufMut) {
        out.put_u8(EVNODE_TX_TYPE_ID);
        let payload_len = self.payload_fields_length(None);
        Header {
            list: true,
            payload_length: payload_len,
        }
        .encode(out);
        self.encode_payload_fields(out, None);
    }

    fn payload_len_for_signature(&self) -> usize {
        1 + Header {
            list: true,
            payload_length: self.payload_fields_length(None),
        }
        .length_with_payload()
    }
}

impl RlpEcdsaEncodableTx for EvNodeTransaction {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.payload_fields_length(self.fee_payer_signature.as_ref())
    }

    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.encode_payload_fields(out, self.fee_payer_signature.as_ref());
    }
}

impl RlpEcdsaDecodableTx for EvNodeTransaction {
    const DEFAULT_TX_TYPE: u8 = EVNODE_TX_TYPE_ID;

    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self {
            chain_id: Decodable::decode(buf)?,
            nonce: Decodable::decode(buf)?,
            max_priority_fee_per_gas: Decodable::decode(buf)?,
            max_fee_per_gas: Decodable::decode(buf)?,
            gas_limit: Decodable::decode(buf)?,
            calls: Decodable::decode(buf)?,
            access_list: Decodable::decode(buf)?,
            fee_payer_signature: decode_optional_signature(buf)?,
        })
    }
}

impl Encodable for EvNodeTransaction {
    fn length(&self) -> usize {
        Header {
            list: true,
            payload_length: self.rlp_encoded_fields_length(),
        }
        .length_with_payload()
    }

    fn encode(&self, out: &mut dyn BufMut) {
        self.rlp_encode(out);
    }
}

impl Decodable for EvNodeTransaction {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(buf)
    }
}

impl Compact for EvNodeTransaction {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: alloy_rlp::bytes::BufMut + AsMut<[u8]>,
    {
        let mut out = Vec::new();
        self.encode(&mut out);
        out.to_compact(buf)
    }

    fn from_compact(buf: &[u8], len: usize) -> (Self, &[u8]) {
        let (bytes, buf) = Vec::<u8>::from_compact(buf, len);
        let mut slice = bytes.as_slice();
        let decoded = Self::decode(&mut slice).expect("valid evnode tx rlp");
        (decoded, buf)
    }
}

impl InMemorySize for Call {
    fn size(&self) -> usize {
        core::mem::size_of::<Self>() + self.input.len()
    }
}

impl InMemorySize for EvNodeTransaction {
    fn size(&self) -> usize {
        let calls_size = self.calls.iter().map(InMemorySize::size).sum::<usize>();
        let access_list_size = self.access_list.size();
        let sponsor_sig_size = self
            .fee_payer_signature
            .map(|_| core::mem::size_of::<Signature>())
            .unwrap_or(0);
        core::mem::size_of::<Self>() + calls_size + access_list_size + sponsor_sig_size
    }
}

impl InMemorySize for EvTxType {
    fn size(&self) -> usize {
        core::mem::size_of::<Self>()
    }
}

impl InMemorySize for EvTxEnvelope {
    fn size(&self) -> usize {
        match self {
            EvTxEnvelope::Ethereum(tx) => tx.size(),
            EvTxEnvelope::EvNode(tx) => tx.size(),
        }
    }
}

impl SignerRecoverable for EvTxEnvelope {
    fn recover_signer(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        match self {
            EvTxEnvelope::Ethereum(tx) => tx.recover_signer(),
            EvTxEnvelope::EvNode(tx) => tx
                .signature()
                .recover_address_from_prehash(&tx.tx().executor_signing_hash())
                .map_err(|_| alloy_consensus::crypto::RecoveryError::new()),
        }
    }

    fn recover_signer_unchecked(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        self.recover_signer()
    }
}

impl TxHashRef for EvTxEnvelope {
    fn tx_hash(&self) -> &B256 {
        match self {
            EvTxEnvelope::Ethereum(tx) => tx.tx_hash(),
            EvTxEnvelope::EvNode(tx) => tx.hash(),
        }
    }
}

impl Compact for EvTxType {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: alloy_rlp::bytes::BufMut + AsMut<[u8]>,
    {
        match self {
            EvTxType::Ethereum(inner) => inner.to_compact(buf),
            EvTxType::EvNode => {
                buf.put_u8(EVNODE_TX_TYPE_ID);
                COMPACT_EXTENDED_IDENTIFIER_FLAG
            }
        }
    }

    fn from_compact(mut buf: &[u8], identifier: usize) -> (Self, &[u8]) {
        match identifier {
            COMPACT_EXTENDED_IDENTIFIER_FLAG => {
                let extended_identifier = buf.get_u8();
                match extended_identifier {
                    EVNODE_TX_TYPE_ID => (Self::EvNode, buf),
                    _ => panic!("Unsupported EvTxType identifier: {extended_identifier}"),
                }
            }
            v => {
                let (inner, buf) = alloy_consensus::TxType::from_compact(buf, v);
                (Self::Ethereum(inner), buf)
            }
        }
    }
}

impl Envelope for EvTxEnvelope {
    fn signature(&self) -> &Signature {
        match self {
            EvTxEnvelope::Ethereum(tx) => tx.signature(),
            EvTxEnvelope::EvNode(tx) => tx.signature(),
        }
    }

    fn tx_type(&self) -> Self::TxType {
        match self {
            EvTxEnvelope::Ethereum(tx) => EvTxType::Ethereum(tx.tx_type()),
            EvTxEnvelope::EvNode(_) => EvTxType::EvNode,
        }
    }
}

impl FromTxCompact for EvTxEnvelope {
    type TxType = EvTxType;

    fn from_tx_compact(buf: &[u8], tx_type: Self::TxType, signature: Signature) -> (Self, &[u8])
    where
        Self: Sized,
    {
        match tx_type {
            EvTxType::Ethereum(inner) => {
                let (tx, buf) = reth_ethereum_primitives::TransactionSigned::from_tx_compact(
                    buf, inner, signature,
                );
                (Self::Ethereum(tx), buf)
            }
            EvTxType::EvNode => {
                let (tx, buf) = EvNodeTransaction::from_compact(buf, buf.len());
                let tx = alloy_consensus::Signed::new_unhashed(tx, signature);
                (Self::EvNode(tx), buf)
            }
        }
    }
}

impl ToTxCompact for EvTxEnvelope {
    fn to_tx_compact(&self, buf: &mut (impl alloy_rlp::bytes::BufMut + AsMut<[u8]>)) {
        match self {
            EvTxEnvelope::Ethereum(tx) => tx.to_tx_compact(buf),
            EvTxEnvelope::EvNode(tx) => {
                tx.tx().to_compact(buf);
            }
        }
    }
}

impl Compact for EvTxEnvelope {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: alloy_rlp::bytes::BufMut + AsMut<[u8]>,
    {
        <Self as CompactEnvelope>::to_compact(self, buf)
    }

    fn from_compact(buf: &[u8], len: usize) -> (Self, &[u8]) {
        <Self as CompactEnvelope>::from_compact(buf, len)
    }
}

impl SignedTransaction for EvTxEnvelope {}

impl reth_primitives_traits::serde_bincode_compat::RlpBincode for EvTxEnvelope {}

impl Compress for EvTxEnvelope {
    type Compressed = Vec<u8>;

    fn compress_to_buf<B: bytes::BufMut + AsMut<[u8]>>(&self, buf: &mut B) {
        let _ = Compact::to_compact(self, buf);
    }
}

impl Decompress for EvTxEnvelope {
    fn decompress(value: &[u8]) -> Result<Self, DatabaseError> {
        let (obj, _) = Compact::from_compact(value, value.len());
        Ok(obj)
    }
}

fn optional_signature_length(value: Option<&Signature>) -> usize {
    match value {
        Some(sig) => sig.as_bytes().as_slice().length(),
        None => 1,
    }
}

fn encode_optional_signature(out: &mut dyn BufMut, value: Option<&Signature>) {
    match value {
        Some(sig) => sig.as_bytes().as_slice().encode(out),
        None => out.put_u8(alloy_rlp::EMPTY_STRING_CODE),
    }
}

fn decode_optional_signature(buf: &mut &[u8]) -> alloy_rlp::Result<Option<Signature>> {
    let bytes = Header::decode_bytes(buf, false)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let raw: [u8; 65] = bytes
        .try_into()
        .map_err(|_| alloy_rlp::Error::UnexpectedLength)?;
    Signature::from_raw_array(&raw)
        .map(Some)
        .map_err(|_| alloy_rlp::Error::Custom("invalid signature bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_eips::eip2930::AccessList;

    fn sample_signature() -> Signature {
        let mut bytes = [0u8; 65];
        bytes[64] = 27;
        Signature::from_raw_array(&bytes).expect("valid test signature")
    }

    fn sample_tx() -> EvNodeTransaction {
        EvNodeTransaction {
            chain_id: 1,
            nonce: 1,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: 2,
            gas_limit: 30_000,
            calls: vec![Call {
                to: TxKind::Create,
                value: U256::from(1),
                input: Bytes::new(),
            }],
            access_list: AccessList::default(),
            fee_payer_signature: None,
        }
    }

    #[test]
    fn executor_signing_hash_ignores_sponsor_fields() {
        let mut tx = sample_tx();
        let base_hash = tx.executor_signing_hash();

        tx.fee_payer_signature = Some(sample_signature());

        assert_eq!(base_hash, tx.executor_signing_hash());
    }

    #[test]
    fn sponsor_signing_hash_binds_executor() {
        let tx = sample_tx();
        let a = Address::from_slice(&[1u8; 20]);
        let b = Address::from_slice(&[2u8; 20]);
        assert_ne!(tx.sponsor_signing_hash(a), tx.sponsor_signing_hash(b));
    }

    #[test]
    fn rlp_roundtrip_with_optional_signature() {
        let mut tx = sample_tx();
        tx.fee_payer_signature = Some(sample_signature());

        let mut out = Vec::new();
        tx.encode(&mut out);
        let mut slice = out.as_slice();
        let decoded = EvNodeTransaction::decode(&mut slice).expect("decode tx");
        assert_eq!(decoded.fee_payer_signature, tx.fee_payer_signature);
    }

    #[test]
    fn decode_optional_signature_none() {
        let mut buf: &[u8] = &[alloy_rlp::EMPTY_STRING_CODE];
        let decoded = decode_optional_signature(&mut buf).expect("decode none signature");
        assert_eq!(decoded, None);
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_optional_signature_rejects_invalid_length() {
        let mut buf: &[u8] = &[0x82, 0x01, 0x02];
        let err = decode_optional_signature(&mut buf).expect_err("invalid length");
        assert_eq!(err, alloy_rlp::Error::UnexpectedLength);
    }
}
