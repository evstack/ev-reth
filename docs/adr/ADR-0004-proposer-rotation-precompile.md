# ADR 0004: Execution-Owned Proposer Rotation Precompile

## Changelog

* 2026-04-27: Initial draft

## Status

DRAFT - Not Implemented

## Abstract

ADR-023 in ev-node moves proposer selection from node-local configuration into the execution
environment. ev-reth therefore needs a deterministic execution-state source for the address that
should sign the next ev-node block.

This ADR proposes a small ev-reth precompile that stores the next proposer address in EVM state and
allows a configured admin to update it through normal transactions. ev-node will query ev-reth after
executing each block and return the selected address through `ExecuteResult.NextProposerAddress`.
This keeps proposer rotation controlled by execution state while avoiding a header format change.

## Context

ev-node previously selected the block proposer from genesis or local node configuration. That makes
sequencer key rotation operationally brittle: every node must agree on the same key transition at the
same time, and rotation cannot naturally be governed by EVM state.

ADR-023 changes this model. The execution layer becomes the authority for proposer rotation:

- `ExecuteTxs` returns the state root after block execution.
- `ExecuteTxs` may also return `NextProposerAddress`, the address expected to sign the next block.
- An empty `NextProposerAddress` means the current proposer remains active.
- `GetExecutionInfo` may expose the current next proposer at startup so ev-node can seed state.

For ev-reth, the proposer selector must be:

- deterministic across all nodes;
- persisted in execution state;
- readable by the ev-node EVM execution adapter after block execution;
- updateable through standard EVM transactions;
- protected by strong access control;
- compatible with existing Engine API payload production.

## Alternatives

### 1. Solidity System Contract

A system contract could be deployed at genesis or during an upgrade. It would store the proposer and
implement access control in Solidity.

* **Pros:** Standard bytecode, ABI, events, and audit tooling. Easy for governance contracts and
  multisigs to integrate with.
* **Cons:** ev-reth still needs a reliable node-side read path for ev-node. Chains must manage
  genesis allocation or upgrade deployment. Access-control bugs become bytecode-level consensus
  state, and changes require contract upgrade patterns.

### 2. Node-Local Configuration

Nodes could configure the active proposer or a proposer schedule locally.

* **Pros:** Simple to implement in ev-node.
* **Cons:** Rejected by ADR-023. Node-local configuration is not deterministic execution state and
  can split the network if operators disagree or rotate at different times.

### 3. Engine API Payload Attribute

ev-node could pass the next proposer through payload attributes and ev-reth could echo it back.

* **Pros:** Minimal EVM changes.
* **Cons:** The proposer would be controlled by the block producer rather than by execution state.
  A malicious proposer could rotate authority without a transaction authorized by governance.

### 4. Header Commitment

ev-node could add a next-proposer field to headers.

* **Pros:** Header-only clients can observe proposer rotation directly.
* **Cons:** This changes signed header encoding and hash chains. ADR-023 explicitly avoids this for
  the first version.

## Decision

We will implement an optional proposer rotation precompile in ev-reth.

The precompile will be located at a reserved address:

```text
0x000000000000000000000000000000000000f101
```

It will expose this Solidity-compatible interface:

```solidity
interface IProposerControl {
    function nextProposer() external view returns (address);
    function setNextProposer(address proposer) external;
    function admin() external view returns (address);
}
```

The precompile stores the configured next proposer in its own account storage. The admin is configured
from the chainspec and may be an EOA, a genesis-deployed proxy, or a governance/multisig contract.

The proposed chainspec fields are:

```json
{
  "config": {
    "evolve": {
      "proposerControlAdmin": "0x1234567890123456789012345678901234567890",
      "proposerControlActivationHeight": 0,
      "initialNextProposer": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
    }
  }
}
```

Field semantics:

- `proposerControlAdmin`: enables the precompile and authorizes proposer updates. The zero address
  disables the precompile.
- `proposerControlActivationHeight`: block height where the precompile becomes callable. Defaults to
  `0` when `proposerControlAdmin` is set.
- `initialNextProposer`: optional initial proposer value written or exposed from genesis. If omitted,
  the precompile starts with no stored proposer and ev-node falls back to genesis proposer at startup.

### Storage Model

The precompile will make its own account non-empty, following the mint precompile pattern, so storage
is not pruned between blocks.

Suggested storage slots:

- `slot 0`: `nextProposer` as a 20-byte address encoded in a 32-byte word.

The admin does not need dynamic storage if it is fixed from chainspec. Returning `admin()` can use the
configured value.

### Write Semantics

`setNextProposer(address proposer)`:

- requires `msg.sender == proposerControlAdmin`;
- rejects the zero address;
- stores `proposer` in `slot 0`;
- emits no EVM log in the first version because revm precompile log emission needs explicit support
  and is not necessary for consensus correctness.

An admin contract can be used as `proposerControlAdmin` to provide multisig or governance-controlled
authorization. In that model, transactions are sent to the admin contract, and the admin contract calls
the precompile.

### Read Semantics

`nextProposer()` returns the stored proposer. If no proposer has been stored, it returns the zero
address.

ev-node treats a zero or empty proposer as "unchanged" per ADR-023. For startup, if ev-reth reports
zero, ev-node falls back to `genesis.proposer_address`.

### ev-node Bridge

ev-reth should expose a small RPC method for the ev-node EVM execution adapter:

```text
evolve_getNextProposer(blockTag) -> address
```

The adapter in `../ev-node/execution/evm` will:

1. Query the current proposer before executing block `N`.
2. Execute block `N` through the existing Engine API flow.
3. Query the proposer at block `N`.
4. Return `ExecuteResult.NextProposerAddress` only when the post-execution proposer is non-zero and
   differs from the pre-execution proposer.
5. Return an empty `NextProposerAddress` when unchanged, preserving ADR-023 compatibility.

`GetExecutionInfo` should query `evolve_getNextProposer(latest)` and return it as
`ExecutionInfo.NextProposerAddress` when non-zero.

The RPC method is a convenience and stability layer for ev-node. It avoids embedding Solidity ABI and
contract-call details in the Go adapter while still deriving the value from execution state.

### Existing Chain Upgrade

Existing chains should activate proposer control with a coordinated future-height upgrade:

1. Choose a future `proposerControlActivationHeight` far enough ahead for all full nodes and the
   current sequencer to upgrade ev-reth.
2. Add `proposerControlAdmin` to the chainspec `config.evolve` section. This should normally be a
   genesis-deployed admin proxy, multisig, or security-council contract address. An EOA is acceptable
   only for development or emergency procedures.
3. Set `initialNextProposer` to the currently expected ev-node proposer. This makes
   `evolve_getNextProposer` return the current signer immediately after activation even before the
   first rotation transaction.
4. Upgrade full nodes first, then the sequencer, before the activation height.
5. After activation, rotate by sending a normal transaction from the admin to
   `0x000000000000000000000000000000000000f101` calling `setNextProposer(newProposer)`.
6. Restart or reconfigure the future sequencer so its ev-node signer key matches `newProposer` before
   it is expected to produce block `N+1`.

Example chainspec addition:

```json
{
  "config": {
    "evolve": {
      "proposerControlAdmin": "0x1234567890123456789012345678901234567890",
      "proposerControlActivationHeight": 20000000,
      "initialNextProposer": "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd"
    }
  }
}
```

For a new chain, `proposerControlActivationHeight` can be `0`. For an existing chain, it should be a
future block. Using a future activation height avoids historical state-transition divergence for
archival nodes replaying blocks before proposer control existed.

### Implementation Details

Implementation should follow the existing mint precompile shape:

- add `crates/ev-precompiles/src/proposer.rs`;
- export the module from `crates/ev-precompiles/src/lib.rs`;
- add `ProposerControlPrecompileSettings` in `crates/ev-revm/src/factory.rs`;
- install the precompile in both EVM factory paths when active for the current block;
- parse chainspec fields in `crates/node/src/config.rs`;
- pass settings through `crates/node/src/executor.rs` and test helpers;
- expose the read RPC from `bin/ev-reth/src/main.rs` or the node RPC module wiring;
- update `../ev-node/execution/evm` to call the read RPC and populate ADR-023 fields.

## Consequences

### Backwards Compatibility

The precompile is optional. Networks without `proposerControlAdmin` keep current behavior and ev-node
receives empty proposer updates.

Existing chains can activate the precompile at a future block using
`proposerControlActivationHeight`. Nodes must upgrade before the activation height. Before activation,
calls to the address follow normal EVM behavior for an empty account.

### Positive

* **Execution-owned rotation:** The signer for the next ev-node block is derived from EVM state.
* **Small consensus surface:** The native logic is limited to one authorized storage write and one
  storage read.
* **Operationally simple:** Key rotation is a normal transaction from the configured admin.
* **Governance compatible:** The admin can be a multisig or governance contract.
* **No header change:** The design aligns with ADR-023 and keeps header encoding unchanged.

### Negative

* **Non-standard precompile:** All compatible ev-reth nodes must implement the same native behavior.
* **Admin compromise risk:** The configured admin can rotate proposer authority to an attacker.
* **Limited event support:** A native precompile may not emit standard logs in the first version, so
  monitoring should query state or transaction traces.
* **Cross-repo change:** Full support requires both ev-reth and the ev-node EVM adapter changes.

### Neutral

* A Solidity system contract remains a viable future replacement if standard events or upgradeable
  in-contract policy become more important than native simplicity.
* The precompile does not decide whether a local node has the matching private key. ev-node still
  checks its configured signer against the expected proposer.

## Further Discussions

Open questions before implementation:

- Should `initialNextProposer` be required when the precompile is enabled, or should genesis fallback
  remain sufficient?
- Should `setNextProposer` allow setting the current proposer again, or reject no-op updates?
- Should the precompile support a two-step rotation such as `proposeNextProposer` and
  `acceptProposer`, or is multisig/admin policy enough?
- Should ev-reth expose `evolve_getNextProposer` only on authenticated RPC, or is public read access
  acceptable because the value is public state?
- Should the first version add log emission support, or should monitoring rely on calls and state
  reads?

## Test Cases

Required test coverage:

- Unit tests for `nextProposer`, `admin`, and `setNextProposer` ABI decoding.
- Authorization tests for admin, non-admin EOA, and admin contract caller.
- Zero-address rejection.
- Storage persistence across blocks.
- Activation-height behavior before and after activation.
- Factory registration tests for both EVM factory paths.
- RPC tests for `evolve_getNextProposer`.
- ev-node EVM adapter tests showing unchanged proposer returns empty and changed proposer returns the
  new address.
- End-to-end test where block `N` updates the proposer and ev-node expects the new signer for block
  `N+1`.

## References

* `../ev-node/docs/adr/adr-023-execution-owned-proposer-rotation.md`
* `../ev-node/core/execution/execution.go`
* `crates/ev-precompiles/src/mint.rs`
* `crates/ev-revm/src/factory.rs`
* `crates/node/src/config.rs`
