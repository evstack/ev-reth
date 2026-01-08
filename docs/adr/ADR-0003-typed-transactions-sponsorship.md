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

## References

* {reference link}
