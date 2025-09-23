# ADR 0001: Fee Handler Architecture

## Changelog

* 2025-09-23: Initial draft.

## Status

DRAFT Not Implemented

> Please have a look at the [PROCESS](./PROCESS.md#adr-status) page.
> Use DRAFT if the ADR is in a draft stage (draft PR) or PROPOSED if it's in review.

## Abstract

The Evolve fork needs a configurable way to redirect execution-layer fees instead of always burning them. This ADR introduces a fee handler subsystem whose canonical configuration lives in the chainspec extras under `ev_reth.feeHandlers`. When a chainspec provides that block, the node eagerly loads it at startup, computes three fee buckets—the EIP-1559 base fee, an L1 data-availability reimbursement, and an optional operator fee—and produces a credit plan that adds those wei values to the configured vault addresses. Credits are applied directly to the post-block state before the payload is sealed, so the resulting state root reflects the redirected balances. Networks that omit the config keep burning fees exactly as today. The design mirrors Optimism’s fee vocabulary (sequencer, L1, operator) so existing dashboards and accounting practices still apply, yet it remains generic enough to plug in alternative fee modes in the future. This approach lets operators express policy through configuration instead of hard coding addresses, allows new fee modes to be added incrementally, and keeps the implementation consistent with Optimism’s economic pipeline while remaining optional for vanilla deployments.

## Context

Sequencing EVM transactions on an Evolve chain has different economic requirements than L1 Ethereum. The default Reth behaviour always burns the base fee and expects the consensus layer to decide what to do with blob pricing, but our evolve chain needs to route those flows toward specific accounts (e.g. sequencer fee vaults or an L1 fee escrow). Operators also want to experiment with alternative fee schedules without shipping a new binary every time. Additionally, Celestia data availability introduces blob fee parameters that must be accounted for even though Reth cannot fetch them yet. Any solution must therefore: (1) keep the legacy burn-only path untouched, (2) expose all tunables via configuration that can be distributed alongside deployment tooling, (3) slot cleanly into the payload builder so that state roots remain correct, and (4) remain testable in isolation. The fee handler crate and the builder integration implement these requirements.

## Alternatives

* Status quo: keep burning all fees and require out-of-band transfers. Rejected because it cannot satisfy sequencer economics.
* Hard-code vault addresses in Rust constants. Rejected because it would need rebuilds for every deployment and risks leaking funds if misconfigured.
* Post-block accounting in an external process. Rejected because it would diverge the state root from the executed block and break consensus guarantees.

## Decision

We will embed fee handler parameters in the chainspec extras, parse them once at payload-builder start-up, and store the resulting typed configuration for later reuse. During payload construction we compute fee totals, derive a credit plan, and mutate the block state via `State::increment_balances` before sealing the block so the header’s state root reflects the credits. If the chainspec omits the block or parsing fails we default to the legacy burn path. We will leave room for additional fee modes (e.g. different DA pricing) by representing mode-specific parameters in an enum, and we maintain an environment override for blob base fee until a Celestia feed is available.

## Consequences

### Backwards Compatibility

Networks whose chainspec omits the `ev_reth.feeHandlers` block (or nodes that explicitly clear it) behave exactly as before. Providing the block changes the resulting state root because fees are credited to vaults instead of being burned; operators must ensure all nodes in a given rollup ship the same chainspec (or override JSON) when enabling fee redirection.

### Positive

* Allows per-deployment fee routing without recompilation.
* Keeps state roots accurate by applying credits before sealing the block.
* Provides an extensible location for future fee policy (additional modes, operator incentives).

### Negative

* Misconfiguration of the chainspec extras (or override JSON) could redirect funds to the zero address or an unintended account.
* Applying credits in the builder adds complexity and requires careful testing to avoid double credits.
* Blob fee inputs still rely on an environment override until automated feeds land.

### Neutral

* When credits sum to zero the system behaves identically to the burn path, aside from log noise.

## Further Discussions

* Replace the environment-based blob fee override with a Celestia fee oracle once available.
* Add validation tooling that lint-checks vault addresses before startup.
* Consider exposing Prometheus metrics for credited wei per bucket.

## Test Cases [optional]

The fee handler crate includes unit coverage in `crates/fee-handlers/tests/compute.rs`; additional integration tests should simulate blocks with and without the configuration enabled.

## References

* `etc/ev-reth-genesis.json`
* `docs/fee-handlers.md`
* `crates/fee-handlers/src`
* `crates/node/src/builder.rs`
