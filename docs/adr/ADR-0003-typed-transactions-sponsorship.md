# ADR 0003: Typed Transactions for Sponsorship (Type 0x76)

## Changelog
* 2026-01-05: Initial draft structure.

## Status
DRAFT — Not Implemented

## Abstract

This ADR proposes a new EIP-2718 typed transaction (0x76) for the EvNode protocol. This transaction type natively supports **gas sponsorship** by explicitly separating the `executor` (identity/nonce provider) from the `fee_payer` (gas provider). This eliminates the need for off-chain meta-transaction relayers or complex contract wrappers, implementing sponsorship directly at the protocol level while maintaining compatibility with Reth's modular architecture.

## Context

Gas sponsorship is a recurring requirement for onboarding users and for product flows that should not require the end user to hold native funds. Today, the only available approaches in Reth are:
1.  **Smart Contract Wallets (ERC-4337):** High gas overhead and complexity.
2.  **Meta-transactions (EIP-712):** Requires specific contract support on the destination.
3.  **Custom Bundles (like Tempo):** Requires off-chain infrastructure to bundle transactions.

EvNode aims to support sponsorship natively. We require a mechanism where a transaction can carry two signatures: one for authorization (execution) and one for payment. Unlike bundled approaches, we propose a discrete transaction type to minimize serialization overhead and simplify the chain processing pipeline.

## Decision

We will implement a custom EIP-2718 transaction type `0x76` (`EvNodeTransaction`) that encodes the execution call plus an optional sponsor authorization.

**Key Architectural Decisions:**

1.  **Dual Signature Scheme:** The transaction supports two signature domains. The Executor signature authorizes the action; the Sponsor signature authorizes the gas payment.
2.  **Sponsor Malleability (Open Sponsorship):** The Executor signs a preimage with an *empty* sponsor field. This allows **any** sponsor to pick up a signed intent and sponsor it.
3.  **Reth Integration:** We will use the `NodeTypes` trait system to inject this primitive. We will not fork `reth-transaction-pool` but will implement a custom `TransactionValidator` to verify sponsor signatures at ingress.
4.  **Persistence:** 0x76 transactions are persisted as part of block bodies using a custom envelope in `EthStorage`.

## Specification

### Transaction Structure

**Type Byte:** `0x76`

The payload contains the following fields, RLP encoded. Field order is consensus-critical:

```rust
pub struct EvNodeTransaction {
    // EIP-1559-like fields
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    pub gas_limit: u64,
    pub to: TxKind,
    pub value: U256,
    pub data: Bytes,
    pub access_list: AccessList,
    // Sponsorship Extensions (Optional)
    pub fee_payer: Option<Address>,
    pub fee_payer_signature: Option<Signature>,
}

```

### Encoding (RLP)

Optional fields MUST be encoded deterministically:

* `fee_payer`: encode `0x80` (nil) when `None`.
* `fee_payer_signature`: encode `0x80` (nil) when `None`.

### Signatures and Hashing

This transaction uses two signature domains to prevent collisions and enable the "Open Sponsorship" model.

1. **Executor Signature** (Domain `0x76`)
* Preimage: `0x76 || rlp(fields...)`
* Constraint: `fee_payer` and `fee_payer_signature` MUST be set to `0x80` (empty) in the RLP stream for this hash.
* *Effect:* The executor authorizes the intent regardless of who pays.


2. **Sponsor Signature** (Domain `0x78`)
* Preimage: `0x78 || rlp(fields...)`
* Constraint: `fee_payer` MUST be the sponsor's address. `fee_payer_signature` remains `0x80`.
* *Effect:* The sponsor binds their address to the specific executor intent.


3. **Transaction Hash** (TxHash)
* `keccak256(0x76 || rlp(fields...))` using the final encoded transaction (including the sponsor signature if present).



### Validity Rules

* **State:** `fee_payer` and `fee_payer_signature` MUST be both present or both absent.
* **Behavior:**
* If sponsorship is absent: Executor pays gas (standard EIP-1559 behavior).
* If sponsorship is present: Sponsor pays gas; Executor remains `from` / `ORIGIN`.


* **Validation:**
* Executor signature MUST be valid for domain `0x76`.
* If present, Sponsor signature MUST be valid for domain `0x78`.



## Implementation Strategy

We will utilize Reth's `NodeTypes` configuration to wire these primitives without modifying core crates.

### 1. Primitives Layer (`crates/ev-primitives`)

* Define `EvTxEnvelope` enum implementing `TransactionEnvelope` and `alloy_rlp` traits.
* Implement custom signing and recovery logic (`recover_executor`, `recover_sponsor`).

```rust
#[derive(Clone, Debug, alloy_consensus::TransactionEnvelope)]
#[envelope(ty = 0x76)]
pub enum EvTxEnvelope {
    // ... Standard variants (0, 1, 2, 3)
    EvNode(EvNodeSignedTx),
}

```

### 2. Node Configuration (`crates/node`)

* **Ingress (Attributes):** Update `attributes.rs` to decode `0x76` payloads using `EvTxEnvelope`.
* **Validation (TxPool):** Implement a custom `TransactionValidator`.
* *Critical:* The validator MUST verify the sponsor signature (if present) before admitting the tx to the pool to prevent DoS attacks.
* *Critical:* Check sponsor balance against `gas_limit * max_fee`.



### 3. Execution Layer (`crates/ev-revm`)

* **Handler:** Extend `ConfigureEvm` or implementation a custom `EvmHandler`.
* **Fee Deduction:** Override the standard fee deduction logic.
* Check `tx.type`. If `0x76` and `fee_payer` is present, debit the `fee_payer` account in the REVM database.
* Otherwise, fallback to standard deduction (debit `caller`).


* **Context:** Map `EvNodeTransaction` to `TxEnv`. Ensure `TxEnv.caller` is always the executor.

## Security Considerations

### Denial of Service (DoS)

Signature recovery is expensive (`ecrecover`).

* **Risk:** An attacker floods the node with valid executor signatures but invalid sponsor signatures.
* **Mitigation:** The `TransactionValidator` in the P2P/RPC ingress layer must strictly validate both signatures before propagation or pooling.

## References

* [EIP-2718: Typed Transaction Envelope](https://eips.ethereum.org/EIPS/eip-2718)
* [Reth Custom Node Example](https://github.com/paradigmxyz/reth/tree/main/examples/custom-node)
* [Tempo Protocol Specifications](https://github.com/tempoxyz/tempo)

```

```