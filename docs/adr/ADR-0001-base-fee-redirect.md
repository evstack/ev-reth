# ADR 0001: Base-Fee Redirect Integration

## Changelog

* 2025-09-24: Reframed ADR to describe the ev-revm base-fee redirect integration and renamed file.
* 2025-09-23: Initial draft covering fee-handler exploration (superseded by this revision).

## Status

DRAFT Implemented

> Please have a look at the [PROCESS](./PROCESS.md#adr-status) page.
> Use DRAFT if the ADR is in a draft stage (draft PR) or PROPOSED if it's in review.

## Abstract

Evolve needs to divert the EIP-1559 base fee away from the burn address and into a configurable sink while keeping the rest of the Reth execution pipeline untouched. Rather than layering post-processing on block results or forking the node, we wrap the existing `EthEvm` factory with an `EvEvm` that installs a lightweight handler. The wrapper intercepts every execution invoked by the payload builder (and all inspector paths), applies an optional credit to the configured sink, and then delegates the remainder of transaction processing to the upstream Reth logic. This keeps the change narrowly scoped, preserves Alloy’s EVM interfaces, and lets operators toggle the redirect with a single chainspec flag.

## Context

Sequencer deployments for Evolve must fund a treasury or sequencer vault with the base fee instead of burning it. Previous experiments introduced a “fee handler” stage that mutated state after the payload builder finished, but that approach drifted from Reth’s execution model, complicated accounting, and risked diverging the state root. We need a solution that:

1. Hooks into the existing EVM creation path without invasive changes to upstream Reth code.
2. Works whether or not the caller enables inspectors or tracing.
3. Preserves Alloy’s `EvmFactory` trait so block builders, RPC servers, and tracing utilities require no changes.
4. Lets operators opt in via configuration, keeping the default burn semantics for everyone else.

## Decision

We introduced a new `ev-revm` crate that mirrors Optimism’s `op-revm` layout but adds an `EvEvm` wrapper. The wrapper:

* Stores the optional `BaseFeeRedirect` policy and a flag indicating whether inspection is active.
* Implements Alloy’s `Evm` trait so higher-level code can continue treating the instance as a standard executor.
* Uses `EvHandler`, a thin derivative of Reth’s `MainnetHandler`, to credit the configured sink during `reward_beneficiary` while delegating all other phases to the upstream handler.

Integration happens in three small steps:

1. `EvEvmFactory` wraps `EthEvmFactory` so every call to `create_evm` (or the inspector variant) returns an `EvEvm` that carries the redirect policy.
2. `with_ev_handler` swaps the factory inside `EthEvmConfig` before the payload builder ever requests an executor.
3. `EvBuilder` mirrors `MainBuilder`, allowing code that builds EVMs from contexts to opt into the wrapper with a single method change.

Configuration remains opt-in: operators add an `ev_reth` block under the chainspec’s `config` section (for example, `{ "config": { ..., "ev_reth": { "baseFeeSink": "0x…" }}}`). On startup the node deserializes that block into `EvolvePayloadBuilderConfig`, converts the optional sink address into a `BaseFeeRedirect`, and hands it to `with_ev_handler`. If the field is absent or fails validation, the wrapper records `None` and the handler leaves the base-fee path untouched.

## Consequences

### Positive

* Minimal surface-area change—callers keep using `EthEvmConfig`, `EvmFactory`, and Alloy trait objects, so no widespread refactors are needed.
* Works with or without inspectors thanks to `EvEvmFactory::create_evm_with_inspector`, preventing accidental bypass when tracing is enabled.
* Executes the redirect in the same handler that already credits the block beneficiary, guaranteeing the state root matches expectations.

### Negative

* The redirect policy currently accepts only a single sink address; expanding to multi-recipient distributions will require a follow-up.
* Base-fee credits depend on the block’s `basefee` as recorded in the execution context. If upstream forks introduce new fee semantics, the handler must be updated in lockstep.
* Because the wrapper is built in Rust, chains that wish to change the default sink still need a deployment pipeline for configuration rather than a runtime API.

### Neutral

* Integrations that bypass `EthEvmFactory` and construct `EthEvm` manually will not pick up the redirect until they adopt `EvEvmFactory`; this is acceptable because our payload builder is the only supported entry point.

## Further Discussions

* Extend `BaseFeeRedirect` to support weighted splits or alternative fee sinks (e.g., contracts) once requirements solidify.
* Add telemetry hooks so operators can monitor credited wei per block.
* Document guardrails for inspectors or custom factories to ensure they propagate the redirect when composing with `EvEvm`.

## Test Cases

* Unit tests cover the redirect logic inside `BaseFeeRedirect` and verify the handler credits both the sink and block beneficiary as expected (`crates/ev-revm/src/base_fee.rs`, `crates/ev-revm/src/factory.rs`).
* Integration smoke tests should execute payload-building runs with and without the `config.ev_reth.baseFeeSink` field present to confirm state roots differ only when the redirect is active.

## References

* `crates/ev-revm/src/evm.rs`
* `crates/ev-revm/src/factory.rs`
* `crates/node/src/builder.rs`
* `docs/fee-handlers.md`
