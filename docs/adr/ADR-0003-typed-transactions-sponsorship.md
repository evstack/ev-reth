# ADR 0003: Typed Transactions for Sponsorship

## Changelog

* 2026-01-05: Initial draft structure.

## Status

DRAFT — Not Implemented

## Abstract

This ADR proposes an EvNode EIP-2718 typed transaction (0x76) with optional
gas sponsorship. It separates the fee payer from the executor while preserving
normal EVM execution semantics. The design defines local primitives and
wrappers and wires a custom `NodeTypes` configuration so the node consumes
those primitives end to end, without modifying reth.

## Context

Gas sponsorship is a recurring requirement for onboarding users and for product
flows that should not require the end user to hold native funds. Today, the only
available approach in reth is to bundle sponsorship logic off-chain or via
custom infrastructure, which increases integration complexity and makes
transaction handling inconsistent across clients.

EIP-2718 typed transactions provide a structured way to extend formats while
remaining backward compatible. The project needs a minimal mechanism to
separate fee payer from executor without changing CALLER/nonce semantics,
remain compatible with existing tooling, and be straightforward to implement in
reth validation and propagation layers.

0x76 transactions can enter via `eth_sendRawTransaction` (txpool) and via
Engine API payload attributes (e.g., from `txpoolExt_getTxs`). They must still
pass explicit pre-execution validation on the Engine API path,
and any forced-inclusion transactions from DA must always be treated as
untrusted. Txpool validation is insufficient; all 0x76 transactions must be
explicitly validated on ingestion/execution paths.

## Decision

We will introduce a new EvNode EIP-2718 typed transaction (0x76) that encodes
the execution call plus an optional sponsor authorization. It is an additional
format (not sponsorship-only); other transaction types remain supported. The
executor is the canonical sender (`from`) and nonce owner; EVM execution
semantics (CALLER) always use the executor. The sponsor only pays fees.
Executor and sponsor signatures use distinct domains and therefore require a
custom signed wrapper and hashing logic.

Implementation will define local primitives/envelopes and wire a custom
`NodeTypes`/`NodePrimitives` configuration so all node components consume those
types without modifying reth crates.

Persistence: 0x76 transactions are persisted as part of block bodies. This
requires a custom envelope type used by `NodeTypes::Primitives` and storage
(`EthStorage<CustomEnvelope, Header>`) plus DB codecs for the custom envelope.

## Specification

### Transaction Type

Introduce a new EIP-2718 typed transaction with type byte `0x76`.

```rust
pub struct EvNodeTransaction {
    // EIP-1559-like fields
    chain_id: u64,
    nonce: u64,
    max_priority_fee_per_gas: u128,
    max_fee_per_gas: u128,
    gas_limit: u64,
    to: TxKind,
    value: U256,
    data: Bytes,
    access_list: AccessList,
    // Sponsorship (optional)
    fee_payer: Option<Address>,
    fee_payer_signature: Option<Signature>,
}
```

### Encoding (RLP)

Field order is consensus-critical and MUST be:

`chain_id, nonce, max_priority_fee_per_gas, max_fee_per_gas, gas_limit, to, value, data, access_list, fee_payer, fee_payer_signature`

Optional fields MUST be encoded deterministically:

- `fee_payer`: encode `0x80` when `None`
- `fee_payer_signature`: encode `0x80` when `None`

### Signatures and Hashing

This transaction uses two signature domains:

- **Executor signature** domain `0x76`
- **Sponsor signature** domain `0x78`

Signing preimages:

- Executor: `0x76 || rlp(fields...)` with `fee_payer = 0x80` and
  `fee_payer_signature = 0x80` (always empty)
- Sponsor: `0x78 || rlp(fields...)` with `fee_payer = sponsor_address` and
  `fee_payer_signature = 0x80`

Transaction hash follows EIP-2718:

- `keccak256(0x76 || rlp(fields...))` using the final encoded transaction
  (including `fee_payer_signature` if present)

### Validity Rules

- `fee_payer` and `fee_payer_signature` MUST be both present or both absent.
- If sponsorship is absent, the executor pays gas (standard EIP-1559 behavior).
- If sponsorship is present, the sponsor pays gas; the executor remains `from`.
- The executor signature MUST be valid for the executor domain.
- If present, the sponsor signature MUST be valid for the sponsor domain and
  bound to the executor transaction contents.

### Execution Semantics

- `from` in RPC and EVM MUST be the executor.
- Nonce ownership is always the executor's nonce.
- Gas accounting and fee deduction MUST be charged to the sponsor when present,
  otherwise to the executor.

### Inclusion Path

This transaction type is accepted via `eth_sendRawTransaction` into txpool,
and via Engine API payload attributes in EvNode (potentially sourced from
txpool indirectly). It must still pass explicit pre-execution validation on
the Engine API ingestion path.
Forced-inclusion transactions from DA bypass txpool and therefore must always
be fully validated before execution.

## Implementation Plan

1. Define local primitives and transaction envelope.
   - Add a new local crate (e.g. `crates/ev-primitives`) to host the transaction
     types and wrappers.
   - Define the `EvNodeTransaction` struct, `EvNodeSignedTx` wrapper, and
     `EvTxEnvelope` enum in that crate, using a custom signed wrapper.
   - Register the new typed transaction with `#[envelope(ty = 0x76)]` and keep
     the consensus field ordering explicit in the struct.
   - Define `EvPrimitives` (or equivalent) and ensure it becomes the node's
     `NodeTypes::Primitives` and storage envelope type (e.g.
     `EthStorage<EvTxEnvelope, EvHeader>`).

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
pub enum EvTxEnvelope {
    #[envelope(ty = 0)]
    Legacy(Signed<TxLegacy>),
    #[envelope(ty = 1)]
    Eip2930(Signed<TxEip2930>),
    #[envelope(ty = 2)]
    Eip1559(Signed<TxEip1559>),
    #[envelope(ty = 3)]
    Eip4844(Signed<TxEip4844>),
    #[envelope(ty = 0x76)]
    EvNode(EvNodeSignedTx),
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
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub to: TxKind,
    pub value: U256,
    pub data: Bytes,
    pub access_list: AccessList,
    // Sponsorship fields (payer is separate, optional capability)
    pub fee_payer: Option<Address>,
    pub fee_payer_signature: Option<Signature>,
}
```

2. Specify encoding + signing preimages (keep deterministic signing).
   - Define the exact RLP field order for `EvNodeTransaction`:
     `chain_id, nonce, max_priority_fee_per_gas, max_fee_per_gas, gas_limit, to,
      value, data, access_list, fee_payer, fee_payer_signature`.
     This order is consensus-critical; if encoding is derived from struct field
     order, the struct must match this ordering exactly.
   - Encode optional fields deterministically:
     - `fee_payer`: always encoded; if `None`, encode `0x80`.
     - `fee_payer_signature`: always encoded; if `None`, encode `0x80`.
   - Executor signature preimage (domain: `0x76`):
     - `0x76 || rlp(fields...)` with `fee_payer = 0x80` and
       `fee_payer_signature = 0x80` regardless of whether a sponsor will sign.
   - Sponsor signature preimage (domain: `0x78`):
     - `0x78 || rlp(fields...)` where `fee_payer` is set to the sponsor address
       and `fee_payer_signature = 0x80`.
   - `tx_hash` uses standard EIP-2718 hashing:
     - `keccak256(0x76 || rlp(fields...))` with the *final* `fee_payer_signature`.
   - Ensure the custom signed type exposes:
     - `executor_signature_hash()` (fee_payer fields empty)
     - `sponsor_signature_hash()` (fee_payer = sponsor address)
     - `recover_executor()` and `recover_sponsor()` as applicable
     - trait implementations required by Reth for pool/consensus encoding
       (`Encodable`, `Decodable`, `Encodable2718`, `Decodable2718`, `Transaction`,
       `TxHashRef`, `InMemorySize`, `IsTyped2718`/`Typed2718`).

3. Optional sponsorship behavior.
   - `fee_payer` and `fee_payer_signature` must be both `None` or both `Some`;
     mixed presence is invalid.
   - If `fee_payer_signature` is `None`, the payer is the executor and validation
     follows the standard EIP-1559 path.
   - If `fee_payer_signature` is `Some`, the payer is the sponsor and the sponsor
     signature must be valid for the sponsor domain and bound to the executor.

4. Add the tx type identifier and envelope mapping (local).
   - Define a local `EvTxType` enum in `crates/ev-primitives` with a `EvNode`
     variant mapped to `0x76`.
   - Ensure the local `EvTxEnvelope` `#[envelope(ty = 0x76)]` derives cover the
     canonical transaction envelope. Pool variants are out of scope.

Example (non-normative):

```rust
pub const EVNODE_TX_TYPE_ID: u8 = 0x76;

pub enum EvTxType {
    Legacy,
    Eip2930,
    Eip1559,
    Eip4844,
    EvNode,
}
```

5. Map the new tx to EVM execution.
   - Define `TxEnv` mapping for executor vs sponsor, including gas price and
     fee fields when a sponsor is present.
   - Ensure `from` in RPC and EVM is always the executor (nonce owner).
   - Add execution logic for the new variant in the block executor and
     receipt builder, including any additional receipt fields.
   - Update the handler that performs balance checks and fee deduction so
     the sponsor (not the executor) pays for gas when sponsorship is present.
     This requires a custom handler or hook that replaces
     `validate_against_state_and_deduct_caller` and `reimburse_caller`
     behavior for the 0x76 variant.
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
            kind: tx.to,
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

6. Persistence and storage codecs.
   - Implement DB codecs for the envelope (`Compress`/`Decompress` and compact
     codecs) so blocks containing 0x76 can be stored and retrieved.

7. Decode in Engine API payloads and validate (no pool).
   - Update the Engine API transaction decoding to use `EvTxEnvelope` 2718
     decoding, recover signer, and preserve the encoded bytes.
   - Add fast, stateless validation for sponsorship fields during payload
     decoding to fail early on malformed or invalid signatures.

Example (non-normative):

```rust
let convert = |encoded: Bytes| {
    let tx = EvTxEnvelope::decode_2718_exact(encoded.as_ref())
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
`PayloadBuilderAttributes::try_new` (the `attributes.transactions` decoding),
and currently uses `TransactionSigned::network_decode`.
This needs to be replaced with `EvTxEnvelope::decode_2718_exact` (or equivalent)
and the builder attributes must store the custom signed/envelope type instead
of `reth_primitives::TransactionSigned`. This implies `EvolveNode` must use
custom `NodeTypes::Primitives` so the payload builder and executor operate on
the same envelope type.

8. Define sponsorship validation and failure modes.
   - Specify the sponsor authorization format, signature verification, and
     constraints (e.g. max fee caps).
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
    let balance = db.balance_of(sponsor)?;
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

9. RPC and receipts.
   - Expose an optional `feePayer` (or `sponsor`) field for 0x76 in
     transaction objects for observability; `from` remains the executor.
   - This requires a custom RPC type layer (e.g., a custom `EthApiBuilder` and
     RPC types bound to the custom primitives). The standard Ethereum RPC
     response structs in reth do not include these fields.
   - If receipts are extended, include the same optional field; otherwise
     receipts remain standard.

## References

* https://github.com/tempoxyz/tempo/blob/main/docs/pages/protocol/transactions/spec-tempo-transaction.mdx
* https://github.com/paradigmxyz/reth/tree/main/examples/custom-node
