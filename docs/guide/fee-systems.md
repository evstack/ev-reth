# Fee Systems Guide: Base Fee Redirect, FeeVault, Native Minting

## Overview

This guide connects three related components that move or manage native token value:

- Base fee redirect: redirects EIP-1559 base fees to a configured sink instead of burning them.
- FeeVault: a contract that accumulates native tokens and can split and bridge them.
- Native minting precompile: a privileged mint/burn interface for controlled supply changes.

These components are independent but commonly deployed together. The base fee redirect is a value transfer, not minting. Native minting is explicit supply change and should remain tightly controlled.

## Base Fee Redirect (execution layer)

**Purpose**: Redirect the EIP-1559 base fee to a sink address (EOA or contract) instead of burning it.

**Mechanics**:

- The EVM handler credits `base_fee_per_gas * gas_used` to the configured sink.
- The redirect only activates at or after `baseFeeRedirectActivationHeight`.
- If no sink is configured, base fee burns proceed as standard Ethereum behavior.

**Chainspec configuration** (inside `config.evolve`):

```json
"evolve": {
  "baseFeeSink": "0x00000000000000000000000000000000000000fe",
  "baseFeeRedirectActivationHeight": 0
}
```

See `docs/adr/ADR-0001-base-fee-redirect.md` for implementation details.

## FeeVault (contract level)

**Purpose**: Accumulate native tokens and split them between a bridge destination and a secondary recipient.

**Mechanics**:

- Receives base fees when `baseFeeSink` is set to the FeeVault address.
- Anyone can trigger `sendToCelestia` (or equivalent) once the minimum threshold is met.
- Splits balance by `bridgeShareBps`, sends the bridge share to `HypNativeMinter`, and transfers the remainder to `otherRecipient`.

**Why it pairs with base fee redirect**: the redirect funnels base fees into the FeeVault automatically, turning burned fees into recoverable value for treasury or bridging.

See `docs/contracts/fee_vault.md` for parameters and deployment details.

## Native Token Minting Precompile

**Purpose**: Provide a privileged, auditable way to mint or burn the native token.

**Mechanics**:

- Precompile address: `0x000000000000000000000000000000000000f100`.
- `mintAdmin` manages the allowlist; both admin and allowlisted accounts can call `mint` and `burn`.
- Activation is gated by `mintPrecompileActivationHeight`.
- Setting `mintAdmin` to the zero address disables the precompile.

**Chainspec configuration** (inside `config.evolve`):

```json
"evolve": {
  "mintAdmin": "0x000000000000000000000000000000000000Ad00",
  "mintPrecompileActivationHeight": 0
}
```

See `docs/adr/ADR-0002-native-minting-precompile.md` for the full interface and security notes.

## How They Fit Together

1. **Base fee redirect** credits base fees to a sink address instead of burning them.
2. **FeeVault** can be that sink, so base fees accumulate in a contract with deterministic split logic.
3. **Native minting** is separate and optional; it is used for controlled supply changes (bootstrapping liquidity, treasury operations), not for redirecting fees.

In other words, base fee redirect and FeeVault are about re-routing existing value, while native minting explicitly changes total supply. Keep those responsibilities separate and limit minting access to minimize systemic risk.

## Suggested Deployment Pattern

- Set `baseFeeSink` to the FeeVault address.
- Use `AdminProxy` as the `mintAdmin` and FeeVault owner if you need a safe, upgradeable admin.
- Activate both features at a planned height for existing networks.

References:

- `docs/contracts/admin_proxy.md`
- `docs/contracts/fee_vault.md`
- `docs/adr/ADR-0001-base-fee-redirect.md`
- `docs/adr/ADR-0002-native-minting-precompile.md`
