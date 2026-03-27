# Fee Systems Guide: Base Fee Redirect, FeeVault, Native Minting

## Overview

This guide connects three related components that move or manage native token value:

- Base fee redirect: redirects EIP-1559 base fees to a configured sink instead of burning them.
- FeeVault: a contract that accumulates native tokens and can split and distribute them.
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

## FeeVault (contract level, optional)

**Purpose**: Accumulate native tokens and split them between two configurable recipients.

FeeVault is **optional**. The base fee redirect works with any address — if fees should go to a single destination, point `baseFeeSink` at an EOA or multisig and skip FeeVault entirely. Use FeeVault when you need automatic on-chain splitting, minimum thresholds, or keeper incentives.

**Mechanics**:

- Receives base fees when `baseFeeSink` is set to the FeeVault address.
- Anyone can trigger `distribute()` once the minimum threshold is met.
- Splits balance by `bridgeShareBps`, sends the bridge share to `bridgeRecipient`, and transfers the remainder to `otherRecipient`.

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

1. **Base fee redirect** credits base fees to a sink address instead of burning them. The sink can be any address (EOA, multisig, or contract).
2. **FeeVault** is one option for that sink when you need automatic splitting between two recipients. If fees go to a single destination, skip it.
3. **Native minting** is separate and optional; it is used for controlled supply changes (bootstrapping liquidity, treasury operations), not for redirecting fees.

In other words, base fee redirect and FeeVault are about re-routing existing value, while native minting explicitly changes total supply. Keep those responsibilities separate and limit minting access to minimize systemic risk.

## Suggested Deployment Patterns

**Simple (no FeeVault):** Set `baseFeeSink` to an EOA or multisig. Fees accumulate there directly.

**With splitting (FeeVault):** Set `baseFeeSink` to the FeeVault address. Configure the split between `bridgeRecipient` and `otherRecipient`. Use `AdminProxy` as the FeeVault owner if you need a safe, upgradeable admin.

Both patterns can be combined with native minting if needed. Activate features at a planned height for existing networks.

References:

- `docs/contracts/admin_proxy.md`
- `docs/contracts/fee_vault.md`
- `docs/adr/ADR-0001-base-fee-redirect.md`
- `docs/adr/ADR-0002-native-minting-precompile.md`
