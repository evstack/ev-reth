# ADR 0005: Dynamic Deployment Permissions Precompile

## Changelog

* 2026-08-25: Accepted and implemented.

## Status

ACCEPTED

## Abstract

Chains need to update deployment permissions without coordinating a hard fork for every membership
change. We add an optional state-backed precompile at `0xF102`. A fixed chainspec admin can add and
remove top-level deployers or temporarily disable enforcement. The genesis `deployAllowlist` is the
initial policy, and legacy chains remain unchanged unless they configure a non-zero admin.

## Context

The existing deployment allowlist is static chainspec data. It is simple and deterministic, but an
operational membership change requires every validator to adopt identical configuration at the same
height. Deployment authorization is consensus-critical, so stale local caches and txpool snapshots
cannot be authoritative once policy becomes mutable. The policy must also survive rollback, replay,
and reorganization exactly like other execution state.

The restriction intentionally covers only top-level `CREATE` transactions. Calls and internal
`CREATE/CREATE2` remain available, so operators must separately control factory contracts if they
need stronger permissioning.

## Alternatives

### Continue using chainspec-only configuration

This has the smallest implementation surface but makes routine membership changes hard forks and
does not provide an emergency pause.

### Deploy a Solidity registry

A registry provides events and familiar tooling, but introduces deployable bytecode, upgrade, and
storage-layout concerns for consensus-critical logic. It also requires a bootstrap allocation or
deployment process.

### Make txpool state authoritative

The txpool can reject stale or currently unauthorized deployments, but mutable permission state may
change before execution. Permanent pool rejection would incorrectly discard transactions that can
become valid. Execution must remain authoritative.

## Decision

We install `IDeployPermissions` at `0x000000000000000000000000000000000000F102`
when a non-zero `deployAllowlistAdmin` is configured and `deployAllowlistActivationHeight` is
reached. The activation defaults to block zero.

Before activation, existing pre-activation behavior applies. At activation, the genesis list is the
baseline. An unset enabled flag means enabled, avoiding a bootstrap transaction. A stored disabled
flag makes top-level deployment fail open while leaving the precompile callable. Re-enabling clears
that flag and restores the preserved policy.

Membership uses domain-separated hashed storage keys and tri-state address entries: unset falls back
to the genesis baseline, allowed adds a non-baseline member, and denied removes a baseline member.
Removing a non-baseline member clears its slot; only removed baseline members retain tombstones. An
encoded active-member count distinguishes an uninitialized count from zero. The active set is capped
at 1024 and excludes the zero address. The precompile account receives sentinel code and nonce so
written storage cannot be pruned as an empty account.

The EVM handler reads the disabled flag and caller override directly through the current execution
database. It bypasses the journal SLOAD path so permission checks do not warm accounts or storage or
change transaction gas. State committed by an earlier transaction in the same block is therefore
visible to later deployments. Reverted transactions and reorganized blocks naturally restore the
prior policy.

Dynamic-mode txpool validation does not permanently reject deployments based on permission state.
Legacy mode retains the existing static rejection. Standard `eth_call` provides inspection; no
custom RPC is added.

## Consequences

### Backwards Compatibility

Chains without a non-zero admin use the exact legacy static behavior. Existing chains enabling the
feature can do so only while their configured `deployAllowlistActivationHeight` is still in the
future. A chain whose static activation already passed needs a separate coordinated consensus
upgrade mechanism; v1 intentionally has no second activation field.

### Positive

* Membership and emergency pause changes no longer require hard forks.
* State transitions are ordered, replayable, revertible, and reorg-safe.
* Disabling enforcement does not destroy policy or prevent recovery transactions.
* The genesis list remains useful as a zero-transaction bootstrap baseline.

### Negative

* A compromised admin can allow arbitrary top-level deployments or fail open enforcement.
* The fixed chainspec admin cannot rotate without a hard fork; production deployments need an
  `AdminProxy`, multisig, or governance contract at that address.
* Native precompile operations emit no events in v1.

### Neutral

* Internal factory deployment remains outside this control surface.
* Batching and native event support are deferred.

## Test Cases

Tests cover default-enabled and disabled behavior, authorization, static-call rejection, baseline
fallback, overrides, idempotence, the member cap, activation, direct database reads without journal
warming, transaction ordering, legacy compatibility, and dynamic txpool admission.

## References

* [Permissioned EVM guide](../guide/permissioned-evm.md)
* [AdminProxy](../contracts/admin_proxy.md)
