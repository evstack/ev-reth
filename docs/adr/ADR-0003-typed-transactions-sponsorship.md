# ADR 0003: Typed Transactions for Sponsorship

## Changelog

* 2026-01-05: Initial draft structure.

## Status

DRAFT Not Implemented

> Please have a look at the [PROCESS](./PROCESS.md#adr-status) page.
> Use DRAFT if the ADR is in a draft stage (draft PR) or PROPOSED if it's in review.

## Abstract

This ADR proposes a simplified way to sponsor transactions in reth by using
typed transactions enabled by EIP-2718. The idea is to define a typed
transaction format that separates the gas payer from the executor so the cost
can be covered without altering the normal execution flow. This reduces
complexity for users and integrations.

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

## Alternatives

TODO

## Decision

> This section describes our response to these forces. It is stated in full
> sentences, with active voice. "We will ..."
We will implement gas sponsorship by introducing a new EIP-2718 typed
transaction in ev-reth. The new type (0x76) encodes both the execution call
and a separate sponsor authorization, enabling a sponsor account to pay fees
while preserving normal EVM execution semantics for the user call. The type is
added to the transaction envelope, validated in the txpool, and executed by
charging the sponsor while the sender remains the call origin.

## Implementation Plan

1. Define the transaction envelope and typed transaction.
   - We will mirror the Tempo-style envelope pattern, extending the envelope
     with a sponsorship transaction type (0x76) and a typed wrapper.
   - The sponsorship transaction is specific to ev-reth and is not a wrapper
     around an existing type: it carries explicit sponsor authorization fields.

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
    /// EvReth sponsorship transaction (type 0x76)
    #[envelope(ty = 0x76, typed = SponsorTransaction)]
    Sponsorship(SponsorSigned),
}

pub struct SponsorTransaction {
    // User/executor call fields (sender remains call origin)
    pub chain_id: u64,
    // Sponsorship fields (payer is separate)
    pub fee_payer_signature: Signature,
    pub fee_token: Address,
}
```

## Consequences

> This section describes the resulting context, after applying the decision. All
> consequences should be listed here, not just the "positive" ones. A particular
> decision may have positive, negative, and neutral consequences, but all of them
> affect the team and project in the future.

### Backwards Compatibility

> All ADRs that introduce backwards incompatibilities must include a section
> describing these incompatibilities and their severity. The ADR must explain
> how the author proposes to deal with these incompatibilities. ADR submissions
> without a sufficient backwards compatibility treatise may be rejected outright.

### Positive

> {positive consequences}

### Negative

> {negative consequences}

### Neutral

> {neutral consequences}

## Further Discussions

> While an ADR is in the DRAFT or PROPOSED stage, this section should contain a
> summary of issues to be solved in future iterations (usually referencing comments
> from a pull-request discussion).
>
> Later, this section can optionally list ideas or improvements the author or
> reviewers found during the analysis of this ADR.

## Test Cases [optional]

Test cases for an implementation are mandatory for ADRs that are affecting consensus
changes. Other ADRs can choose to include links to test cases if applicable.

## References

* {reference link}
