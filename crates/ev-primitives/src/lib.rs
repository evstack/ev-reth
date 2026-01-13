//! EV-specific primitive types, including the EvNode 0x76 transaction.

use alloy_consensus::{
    transaction::RlpEcdsaDecodableTx, transaction::RlpEcdsaEncodableTx, SignableTransaction,
    Transaction, TransactionEnvelope,
};
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{keccak256, Address, Bytes, Signature, TxKind, B256, U256};
use alloy_rlp::{BufMut, Decodable, Encodable, Header, RlpDecodable, RlpEncodable};

/// EIP-2718 transaction type for EvNode batch + sponsorship.
pub const EVNODE_TX_TYPE_ID: u8 = 0x76;
/// Signature domain for sponsor authorization.
pub const EVNODE_SPONSOR_DOMAIN: u8 = 0x78;

/// Single call entry in an EvNode transaction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, RlpEncodable, RlpDecodable)]
pub struct Call {
    /// Destination (CALL or CREATE).
    pub to: TxKind,
    /// ETH value.
    pub value: U256,
    /// Calldata.
    pub input: Bytes,
}

/// EvNode batch + sponsorship transaction payload.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EvNodeTransaction {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub calls: Vec<Call>,
    pub access_list: AccessList,
    pub fee_payer: Option<Address>,
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
    Ethereum(alloy_consensus::TxEnvelope),
    /// EvNode typed transaction.
    #[envelope(ty = 0x76)]
    EvNode(EvNodeSignedTx),
}

impl EvNodeTransaction {
    /// Returns the executor signing hash (domain 0x76, empty sponsor fields).
    pub fn executor_signing_hash(&self) -> B256 {
        let payload = self.encoded_payload(None, None);
        let mut preimage = Vec::with_capacity(1 + payload.len());
        preimage.push(EVNODE_TX_TYPE_ID);
        preimage.extend_from_slice(&payload);
        keccak256(preimage)
    }

    /// Returns the sponsor signing hash (domain 0x78, sponsor address bound).
    pub fn sponsor_signing_hash(&self, fee_payer: Address) -> B256 {
        let payload = self.encoded_payload(Some(fee_payer), None);
        let mut preimage = Vec::with_capacity(1 + payload.len());
        preimage.push(EVNODE_SPONSOR_DOMAIN);
        preimage.extend_from_slice(&payload);
        keccak256(preimage)
    }

    /// Recovers the executor address from the provided signature.
    pub fn recover_executor(&self, signature: &Signature) -> Result<Address, alloy_primitives::SignatureError> {
        signature.recover_address_from_prehash(&self.executor_signing_hash())
    }

    /// Recovers the sponsor address from the provided signature and fee payer.
    pub fn recover_sponsor(
        &self,
        fee_payer: Address,
        signature: &Signature,
    ) -> Result<Address, alloy_primitives::SignatureError> {
        signature.recover_address_from_prehash(&self.sponsor_signing_hash(fee_payer))
    }

    fn first_call(&self) -> Option<&Call> {
        self.calls.first()
    }

    fn encoded_payload(
        &self,
        fee_payer: Option<Address>,
        fee_payer_signature: Option<&Signature>,
    ) -> Vec<u8> {
        let payload_len = self.payload_fields_length(fee_payer, fee_payer_signature);
        let mut out = Vec::with_capacity(Header { list: true, payload_length: payload_len }.length_with_payload());
        Header { list: true, payload_length: payload_len }.encode(&mut out);
        self.encode_payload_fields(&mut out, fee_payer, fee_payer_signature);
        out
    }

    fn payload_fields_length(
        &self,
        fee_payer: Option<Address>,
        fee_payer_signature: Option<&Signature>,
    ) -> usize {
        self.chain_id.length()
            + self.nonce.length()
            + self.max_priority_fee_per_gas.length()
            + self.max_fee_per_gas.length()
            + self.gas_limit.length()
            + self.calls.length()
            + self.access_list.length()
            + optional_address_length(fee_payer.as_ref())
            + optional_signature_length(fee_payer_signature)
    }

    fn encode_payload_fields(
        &self,
        out: &mut dyn BufMut,
        fee_payer: Option<Address>,
        fee_payer_signature: Option<&Signature>,
    ) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.max_priority_fee_per_gas.encode(out);
        self.max_fee_per_gas.encode(out);
        self.gas_limit.encode(out);
        self.calls.encode(out);
        self.access_list.encode(out);
        encode_optional_address(out, fee_payer.as_ref());
        encode_optional_signature(out, fee_payer_signature);
    }
}

impl Transaction for EvNodeTransaction {
    fn chain_id(&self) -> Option<alloy_primitives::ChainId> {
        Some(self.chain_id.into())
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
        base_fee.map_or(self.max_fee_per_gas, |base_fee| {
            let tip = self.max_fee_per_gas.saturating_sub(base_fee as u128);
            if tip > self.max_priority_fee_per_gas {
                self.max_priority_fee_per_gas + base_fee as u128
            } else {
                self.max_fee_per_gas
            }
        })
    }

    fn is_dynamic_fee(&self) -> bool {
        true
    }

    fn kind(&self) -> TxKind {
        self.first_call().map(|call| call.to).unwrap_or(TxKind::Create)
    }

    fn is_create(&self) -> bool {
        matches!(self.first_call().map(|call| call.to), Some(TxKind::Create))
    }

    fn value(&self) -> U256 {
        self.first_call().map(|call| call.value).unwrap_or_default()
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
        self.chain_id = chain_id.into();
    }

    fn encode_for_signing(&self, out: &mut dyn BufMut) {
        out.put_u8(EVNODE_TX_TYPE_ID);
        let payload_len = self.payload_fields_length(None, None);
        Header { list: true, payload_length: payload_len }.encode(out);
        self.encode_payload_fields(out, None, None);
    }

    fn payload_len_for_signature(&self) -> usize {
        1 + Header { list: true, payload_length: self.payload_fields_length(None, None) }.length_with_payload()
    }
}

impl RlpEcdsaEncodableTx for EvNodeTransaction {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.payload_fields_length(self.fee_payer, self.fee_payer_signature.as_ref())
    }

    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.encode_payload_fields(out, self.fee_payer, self.fee_payer_signature.as_ref());
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
            fee_payer: decode_optional_address(buf)?,
            fee_payer_signature: decode_optional_signature(buf)?,
        })
    }
}

impl Encodable for EvNodeTransaction {
    fn length(&self) -> usize {
        Header { list: true, payload_length: self.rlp_encoded_fields_length() }.length_with_payload()
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

fn optional_address_length(value: Option<&Address>) -> usize {
    match value {
        Some(addr) => addr.length(),
        None => 1,
    }
}

fn optional_signature_length(value: Option<&Signature>) -> usize {
    match value {
        Some(sig) => sig.as_bytes().as_slice().length(),
        None => 1,
    }
}

fn encode_optional_address(out: &mut dyn BufMut, value: Option<&Address>) {
    match value {
        Some(addr) => addr.encode(out),
        None => out.put_u8(alloy_rlp::EMPTY_STRING_CODE),
    }
}

fn encode_optional_signature(out: &mut dyn BufMut, value: Option<&Signature>) {
    match value {
        Some(sig) => sig.as_bytes().as_slice().encode(out),
        None => out.put_u8(alloy_rlp::EMPTY_STRING_CODE),
    }
}

fn decode_optional_address(buf: &mut &[u8]) -> alloy_rlp::Result<Option<Address>> {
    let bytes = Header::decode_bytes(buf, false)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() != 20 {
        return Err(alloy_rlp::Error::UnexpectedLength);
    }
    Ok(Some(Address::from_slice(bytes)))
}

fn decode_optional_signature(buf: &mut &[u8]) -> alloy_rlp::Result<Option<Signature>> {
    let bytes = Header::decode_bytes(buf, false)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let raw: [u8; 65] = bytes.try_into().map_err(|_| alloy_rlp::Error::UnexpectedLength)?;
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
            calls: vec![Call { to: TxKind::Create, value: U256::from(1), input: Bytes::new() }],
            access_list: AccessList::default(),
            fee_payer: None,
            fee_payer_signature: None,
        }
    }

    #[test]
    fn executor_signing_hash_ignores_sponsor_fields() {
        let mut tx = sample_tx();
        let base_hash = tx.executor_signing_hash();

        tx.fee_payer = Some(Address::ZERO);
        tx.fee_payer_signature = Some(sample_signature());

        assert_eq!(base_hash, tx.executor_signing_hash());
    }

    #[test]
    fn sponsor_signing_hash_binds_fee_payer() {
        let tx = sample_tx();
        let a = Address::from_slice(&[1u8; 20]);
        let b = Address::from_slice(&[2u8; 20]);
        assert_ne!(tx.sponsor_signing_hash(a), tx.sponsor_signing_hash(b));
    }

    #[test]
    fn rlp_roundtrip_with_optional_signature() {
        let mut tx = sample_tx();
        tx.fee_payer = Some(Address::from_slice(&[3u8; 20]));
        tx.fee_payer_signature = Some(sample_signature());

        let mut out = Vec::new();
        tx.encode(&mut out);
        let mut slice = out.as_slice();
        let decoded = EvNodeTransaction::decode(&mut slice).expect("decode tx");
        assert_eq!(decoded.fee_payer, tx.fee_payer);
        assert_eq!(decoded.fee_payer_signature, tx.fee_payer_signature);
    }

    #[test]
    fn decode_optional_address_none() {
        let mut buf: &[u8] = &[alloy_rlp::EMPTY_STRING_CODE];
        let decoded = decode_optional_address(&mut buf).expect("decode none address");
        assert_eq!(decoded, None);
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_optional_signature_none() {
        let mut buf: &[u8] = &[alloy_rlp::EMPTY_STRING_CODE];
        let decoded = decode_optional_signature(&mut buf).expect("decode none signature");
        assert_eq!(decoded, None);
        assert!(buf.is_empty());
    }

    #[test]
    fn decode_optional_address_rejects_invalid_length() {
        let mut data = vec![0u8; 19];
        data.insert(0, 0x93);
        let mut buf: &[u8] = &data;
        let err = decode_optional_address(&mut buf).expect_err("invalid length");
        assert_eq!(err, alloy_rlp::Error::UnexpectedLength);
    }

    #[test]
    fn decode_optional_signature_rejects_invalid_length() {
        let mut buf: &[u8] = &[0x82, 0x01, 0x02];
        let err = decode_optional_signature(&mut buf).expect_err("invalid length");
        assert_eq!(err, alloy_rlp::Error::UnexpectedLength);
    }
}
