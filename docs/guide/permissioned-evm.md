# Permissioned EVM Guide: Contract Deployment Allowlist

## Overview

This guide covers static and state-backed deployment permissions. They restrict top-level contract
creation transactions to approved callers. They do not restrict regular calls and are not a full
transaction allowlist.

## Deploy Allowlist (execution layer)

**Purpose**: Restrict contract deployment to a known set of accounts.

**Mechanics**:

- Enforcement happens in the EVM handler before execution.
- Only top-level contract creation transactions are checked.
- Contract-to-contract `CREATE/CREATE2` is still allowed (by design).
- If no allowlist or dynamic admin is configured, behavior matches standard Ethereum.

**Chainspec configuration** (inside `config.evolve`):

```json
"evolve": {
  "deployAllowlist": [
    "0xYourDeployerAddressHere",
    "0xAnotherDeployerAddressHere"
  ],
  "deployAllowlistActivationHeight": 0
}
```

## Activation and Validation Rules

- If `deployAllowlist` is set and `deployAllowlistActivationHeight` is omitted, activation
  defaults to `0`.
- In legacy static mode, an empty or missing list leaves deployment unrestricted.
- Duplicate entries or the zero address are rejected at startup.
- The list is capped at 1024 addresses.

## Dynamic Deployment Permissions

Set `deployAllowlistAdmin` to enable the state-backed precompile at
`0x000000000000000000000000000000000000F102`:

```json
"evolve": {
  "deployAllowlist": [
    "0xInitialDeployerAddress"
  ],
  "deployAllowlistActivationHeight": 20000000,
  "deployAllowlistAdmin": "0xAdminProxyOrGovernanceAddress"
}
```

`deployAllowlistActivationHeight` defaults to `0` when omitted. Before that block, deployment
behavior is unchanged. At and after that block, the genesis list is the dynamic baseline and
state-backed enforcement is enabled by default.

A configured empty baseline is intentionally different from legacy mode: it denies every top-level
deployment while enforcement is enabled. Setting the admin to zero is the same as omitting it and
preserves legacy static behavior.

### Interface

```solidity
interface IDeployPermissions {
    function addDeployer(address account) external;
    function removeDeployer(address account) external;
    function setEnabled(bool enabled) external;
    function isDeployerAllowed(address account) external view returns (bool);
    function isEnabled() external view returns (bool);
    function deployerCount() external view returns (uint256);
    function admin() external view returns (address);
}
```

Only the fixed chainspec admin can mutate state. `addDeployer`, `removeDeployer`, and `setEnabled`
are idempotent and reject unauthorized callers; member mutations also reject the zero address.
There can be at most 1024 active deployers.

`setEnabled(false)` is a reversible fail-open pause: every top-level deployment is allowed. It does
not disable the precompile or discard the allowlist. The admin can edit membership while enforcement
is disabled and then call `setEnabled(true)` to enforce the updated policy. `isDeployerAllowed`
reports membership in that preserved policy; combine it with `isEnabled` to determine current
enforcement behavior.

Use standard `eth_call` against `F102` for reads. No custom RPC method is registered. The precompile
emits no native events in v1.

## Security and Limitations

- This is not a general permissioned chain; it only gates top-level contract creation.
- Non-allowlisted EOAs can still deploy contracts via existing factory contracts if those
  factories allow it.
- If you need stricter control, only deploy factories with explicit access control and avoid
  deploying open factories.

## Operational Notes

- Without `deployAllowlistAdmin`, changes require a chainspec update and coordinated activation.
- With an admin, membership and enforcement state are ordinary execution state and follow normal
  transaction rollback and chain-reorganization semantics.
- For production, point the fixed admin at an `AdminProxy`, multisig, or governance contract. Admin
  rotation should happen behind that contract; changing the chainspec admin requires a hard fork.
- Existing networks can opt in only if their configured `deployAllowlistActivationHeight` is still
  in the future. Networks whose static activation has already passed need a separate coordinated
  consensus upgrade mechanism; this version intentionally does not expose a second activation
  height.

References:

- `crates/node/src/config.rs`
- `crates/ev-revm/src/handler.rs`
- `crates/ev-precompiles/src/deploy_permissions.rs`
- `docs/adr/ADR-0005-dynamic-deployment-permissions.md`
