# Permissioned EVM Guide: Contract Deployment Allowlist

## Overview

This guide covers the deploy allowlist: a chainspec-controlled guardrail that restricts
top-level contract creation transactions to a set of approved EOAs. It does not restrict
regular call transactions and is not a full transaction allowlist.

## Deploy Allowlist (execution layer)

**Purpose**: Restrict contract deployment to a known set of EOAs.

**Mechanics**:

- Enforcement happens in the EVM handler before execution.
- Only top-level contract creation transactions are checked.
- Contract-to-contract `CREATE/CREATE2` is still allowed (by design).
- If no allowlist is configured, behavior matches standard Ethereum.
- An empty allowlist is treated as disabled and allows all deployers.

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
- If the allowlist is empty or missing, contract deployment is unrestricted (treated as disabled).
- Duplicate entries or the zero address are rejected at startup.
- The list is capped at 1024 addresses.

## Security and Limitations

- This is not a general permissioned chain; it only gates top-level contract creation.
- Non-allowlisted EOAs can still deploy contracts via existing factory contracts if those
  factories allow it.
- If you need stricter control, only deploy factories with explicit access control and avoid
  deploying open factories.

## Operational Notes

- The allowlist is static; changes require a chainspec update and node restart.
- For existing networks, use an activation height to coordinate rollouts.

References:

- `crates/node/src/config.rs`
- `crates/ev-revm/src/handler.rs`
