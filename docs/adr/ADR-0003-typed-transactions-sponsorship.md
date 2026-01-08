# ADR 0003: Typed Transactions for Sponsorship

## Changelog

* 2026-01-05: Initial draft structure.

## Status

DRAFT Not Implemented

## Abstract

This ADR proposes a canonical EvNode transaction type that includes gas
sponsorship as a first-class capability, using EIP-2718 typed transactions.
The idea is to define a typed transaction format that separates the gas payer
from the executor so the cost can be covered without altering the normal
execution flow. This reduces complexity for users and integrations.

## Context

Gas sponsorship is a recurring requirement for onboarding users and for product
flows that should not require the end user to hold native funds. Today, the only
available approach in reth is to bundle sponsorship logic off-chain or via
custom infrastructure, which increases integration complexity and makes
transaction handling inconsistent across clients.

EIP-2718 introduces typed transactions, providing a structured way to extend
transaction formats while keeping backward compatibility with existing
processing pipelines. This creates an opportunity to standardize a sponsorship
mechanism within the transaction itself rather than relying on external
conventions.

The project needs a minimal, explicit mechanism to separate the gas payer from
the executor, without changing the execution semantics of the underlying call.
At the same time, it must remain compatible with existing tooling, avoid
breaking current transaction flows, and be straightforward to implement in
reth's transaction validation and propagation layers.

## Decision

We will introduce a new canonical EvNode transaction type using EIP-2718
typed transactions. This type (0x76) encodes both the execution call and an
optional sponsor authorization, enabling a sponsor account to pay fees while
preserving normal EVM execution semantics for the user call. It is not a
"sponsorship-only" transaction; it is the primary EvNode transaction format
and sponsorship is an optional capability. The type is added to the transaction
envelope. The transaction itself uses the standard secp256k1 signature wrapper
(`Signed<T>`), so we do not introduce a custom signed wrapper type.

## Implementation Plan

1. Define the consensus transaction envelope and type.
   - Add a crate-local envelope enum that derives `TransactionEnvelope` and
     declares the tx type name (e.g. `EvRethTxType`) for all supported variants.
   - Use `#[envelope(ty = 0x76]` to register the
     custom typed transaction and ensure the type byte does not collide.
   - Keep the custom transaction as a concrete struct (not a wrapper), so its
     fields and ordering are explicitly defined at the consensus layer.
   - The user signature remains the standard `Signed<T>` wrapper (secp256k1).

```rust
#[derive(Clone, Debug, alloy_consensus::TransactionEnvelope)]
#[envelope(
    tx_type_name = EvRethTxType,
    typed = EvRethTypedTransaction,
    arbitrary_cfg(any(test, feature = "arbitrary")),
    serde_cfg(feature = "serde")
)]
#[cfg_attr(test, reth_codecs::add_arbitrary_tests(compact, rlp))]
#[expect(clippy::large_enum_variant)]
pub enum EvRethTxEnvelope {
    #[envelope(ty = 0)]
    Legacy(Signed<TxLegacy>),
    #[envelope(ty = 1)]
    Eip2930(Signed<TxEip2930>),
    #[envelope(ty = 2)]
    Eip1559(Signed<TxEip1559>),
    #[envelope(ty = 3)]
    Eip4844(Signed<TxEip4844>),
    #[envelope(ty = 0x76]
    EvNode(Signed<EvNodeTransaction>),
}

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    reth_codecs::Compact,
)]
#[serde(rename_all = "camelCase")]
pub struct EvNodeTransaction {
    // These mirror EIP-1559 fields to stay compatible with the standard.
    pub chain_id: u64,
    pub nonce: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub access_list: AccessList,
    // Sponsorship fields (payer is separate, optional capability)
    pub fee_payer_signature: Option<Signature>,
    pub fee_token: Option<Address>,
}
```

2. Specify the transaction encoding and signing payload.
   - Define the exact RLP field ordering for `EvNodeTransaction`, including
     how optional sponsorship fields are represented.
   - Declare the signing payload for both the executor signature and the
     optional sponsor authorization. This must include the EIP-2718 type byte
     and the correct chain_id replay protection.
   - Document how `tx_hash` is computed and ensure the hashing matches
     `Signed<T>` expectations in reth/alloy.

Example (non-normative):

```rust
impl RlpEcdsaEncodableTx for EvNodeTransaction {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length()
            + self.nonce.length()
            + self.max_priority_fee_per_gas.length()
            + self.max_fee_per_gas.length()
            + self.gas_limit.length()
            + self.to.length()
            + self.value.length()
            + self.data.length()
            + self.access_list.length()
            + self.fee_payer_signature.length()
            + self.fee_token.length()
    }

    fn rlp_encode_fields(&self, out: &mut dyn alloy_rlp::BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.max_priority_fee_per_gas.encode(out);
        self.max_fee_per_gas.encode(out);
        self.gas_limit.encode(out);
        self.to.encode(out);
        self.value.encode(out);
        self.data.encode(out);
        self.access_list.encode(out);
        self.fee_payer_signature.encode(out);
        self.fee_token.encode(out);
    }
}

impl SignableTransaction<Signature> for EvNodeTransaction {
    fn encode_for_signing(&self, out: &mut dyn alloy_rlp::BufMut) {
        out.put_u8(Self::tx_type().ty());
        self.encode(out);
    }
}
```

3. Add the tx type identifier and compact encoding.
   - Register the new type id in the custom `TxType` enum and compact codec
     (extended identifier if needed), so storage/network encoding works.
   - Ensure `TransactionEnvelope` derives cover both the canonical and pooled
     variants without conflicting type ids.

Code-level implications:
   - Add a `EvNode`/`EvRethTxType` variant that maps to `0x76`.
   - Implement `Compact` for the `TxType` enum so `0x76` round-trips through the
     compact codec (use `COMPACT_EXTENDED_IDENTIFIER_FLAG` if required).
   - Register `#[envelope(ty = 0x76)]` on both the canonical transaction
     envelope and the pooled transaction envelope, so 2718 decoding matches
     the compact encoding.

Example (non-normative):

```rust
pub const EVNODE_TX_TYPE_ID: u8 = 0x76;

impl Compact for EvRethTxType {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: BufMut + AsMut<[u8]>,
    {
        match self {
            Self::EvNode => {
                buf.put_u8(EVNODE_TX_TYPE_ID);
                COMPACT_EXTENDED_IDENTIFIER_FLAG
            }
            Self::Op(ty) => ty.to_compact(buf),
        }
    }

    fn from_compact(mut buf: &[u8], identifier: usize) -> (Self, &[u8]) {
        match identifier {
            COMPACT_EXTENDED_IDENTIFIER_FLAG => {
                let extended_identifier = buf.get_u8();
                match extended_identifier {
                    EVNODE_TX_TYPE_ID => (Self::EvNode, buf),
                    _ => panic!("Unsupported TxType identifier: {extended_identifier}"),
                }
            }
            v => {
                let (inner, buf) = EvRethTxType::from_compact(buf, v);
                (inner, buf)
            }
        }
    }
}
```

4. Map the new tx to EVM execution.
   - Define `TxEnv` mapping for executor vs sponsor, including gas price and
     fee fields when a sponsor is present.
   - Add execution logic for the new variant in the block executor and
     receipt builder, including any additional receipt fields.
   - If sponsorship requires execution-time data beyond the standard
     `revm::context::TxEnv`, introduce a custom TxEnv; otherwise map directly
     into the standard `TxEnv`.

Example (non-normative):

```rust
impl FromRecoveredTx<EvNodeTransaction> for TxEnv {
    fn from_recovered_tx(tx: &EvNodeTransaction, caller: Address) -> Self {
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: tx.gas_limit,
            gas_price: tx.max_fee_per_gas,
            gas_priority_fee: Some(tx.max_priority_fee_per_gas),
            kind: TxKind::Call(tx.to),
            value: tx.value,
            data: tx.data.clone(),
            access_list: tx.access_list.clone(),
            chain_id: Some(tx.chain_id),
            ..Default::default()
        }
    }
}

match tx.tx() {
    EvRethTxEnvelope::EvNode(ev_tx) => {
        // Resolve sponsor vs executor and apply fee accounting.
        let sponsor = resolve_fee_payer(ev_tx.tx(), *tx.signer())?;
        execute_with_fee_payer(ev_tx, sponsor)?;
    }
    _ => { /* existing paths */ }
}
```

5. Decode in Engine API payloads and validate.
   - Update the payload transaction iterator to decode the custom type using
     2718 decoding, recover signer, and preserve the encoded bytes.
   - Add fast, stateless validation for sponsorship fields during payload
     decoding to fail early on malformed or invalid signatures.

Example (non-normative):

```rust
let convert = |encoded: Bytes| {
    let tx = EvRethTxEnvelope::decode_2718_exact(encoded.as_ref())
        .map_err(Into::into)
        .map_err(PayloadError::Decode)?;
    let signer = tx.try_recover().map_err(NewPayloadError::other)?;
    // Optional: fast, stateless validation before execution.
    validate_sponsor_fields(tx.tx(), signer).map_err(NewPayloadError::other)?;
    Ok::<_, NewPayloadError>(WithEncoded::new(encoded, tx.with_signer(signer)))
};
```

Note: in this repo, the Engine API decode/validation currently happens in
`crates/node/src/attributes.rs` within
`PayloadBuilderAttributes::try_new` (the `attributes.transactions` decoding).

6. Define sponsorship validation and failure modes.
   - Specify the sponsor authorization format, signature verification, and
     constraints (e.g. max fee caps, allowed fee tokens).
   - Define stateful validation and exact behavior when sponsor auth is
     missing/invalid or sponsor balance is insufficient (reject vs fallback
     to executor payment).

Example (non-normative):

```rust
// Stateful validation can live just before execution or inside the EVM handler.
fn validate_sponsor_state(
    db: &impl StateProvider,
    tx: &EvNodeTransaction,
    sponsor: Address,
) -> Result<(), ValidationError> {
    let fee_token = tx.fee_token.unwrap_or(DEFAULT_FEE_TOKEN);
    let balance = db.balance_of(fee_token, sponsor)?;
    let max_cost = tx.gas_limit as u128 * tx.max_fee_per_gas;
    if balance < max_cost.into() {
        return Err(ValidationError::InsufficientSponsorBalance);
    }
    Ok(())
}
```

Note: stateful validation will be enforced inside the execution handler in
`crates/ev-revm/src/handler.rs` so rules apply consistently at runtime. A
builder-level pre-check is optional.

## References

* https://github.com/tempoxyz/tempo
* https://github.com/tempoxyz/tempo/blob/main/docs/pages/protocol/transactions/spec-tempo-transaction.mdx
