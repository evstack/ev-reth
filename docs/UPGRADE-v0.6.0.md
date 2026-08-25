# Upgrade Guide: v0.6.0

This guide covers rollout of the optional dynamic deployment-permissions precompile. Existing
networks that do not configure `deployAllowlistAdmin` require no chainspec changes and retain their
static deployment behavior.

## Dynamic Deployment Permissions

The precompile is installed at `0x000000000000000000000000000000000000F102` at the configured
activation block. Add these fields inside `config.evolve`:

```json
"deployAllowlist": [
  "0xInitialDeployerAddress"
],
"deployAllowlistActivationHeight": 0,
"deployAllowlistAdmin": "0xAdminProxyOrGovernanceAddress",
"deployAllowlistPrecompileActivationHeight": 20000000
```

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `deployAllowlist` | `address[]` | empty | Genesis baseline for dynamic membership. Empty means deny-all while enabled. |
| `deployAllowlistActivationHeight` | `u64` | `0` for a non-empty list | Activation for legacy static enforcement before dynamic activation. |
| `deployAllowlistAdmin` | `address` | -- | Enables dynamic mode and authorizes mutations. Zero or omitted preserves legacy behavior. |
| `deployAllowlistPrecompileActivationHeight` | `u64` | `0` | Block where `F102` and dynamic enforcement activate. |

When the baseline is non-empty, the precompile activation height must be at or after the static
activation height. The feature is enabled by default at activation. Calling `setEnabled(false)`
allows all top-level deployments until the admin re-enables the preserved policy.

## Existing-Network Rollout

1. Choose a future activation height with enough time for every validator and sequencer to upgrade.
2. Set `deployAllowlistAdmin` to an existing `AdminProxy`, multisig, or governance contract. Do not
   use a disposable EOA for production authority.
3. Keep `deployAllowlist` equal to the policy that should be active at the transition.
4. Distribute the identical chainspec and upgraded binary to every validating node before activation.
5. At activation, verify `isEnabled()`, `deployerCount()`, `admin()`, and representative
   `isDeployerAllowed(address)` calls with standard `eth_call`.
6. Exercise add/remove and pause transactions only after activation; calls sent before activation
   are ordinary empty-account calls and write no permission state.

Do not rely on txpool rejection as a dynamic policy check. A transaction admitted while allowed may
be rejected at execution after a policy change, and a transaction admitted while currently denied
may become valid before execution. Block execution is authoritative.

## Rollback

Before activation, roll back by restoring the previous binary and chainspec. After activation has
produced blocks, changing or removing the feature requires a coordinated consensus upgrade; do not
silently move the activation height. Operationally, the configured admin can call `setEnabled(false)`
to fail open deployment while preserving membership for later recovery.

## Checklist

- [ ] All validators use the same admin, baseline, and activation heights
- [ ] Dynamic activation does not precede non-empty static-list activation
- [ ] The admin contract and its recovery process are tested
- [ ] Every validating node upgrades before the activation block
- [ ] Read calls at `F102` are verified at activation
- [ ] Monitoring distinguishes policy membership from the global enabled flag

See [ADR 0005](adr/ADR-0005-dynamic-deployment-permissions.md) and the
[permissioned EVM guide](guide/permissioned-evm.md).
