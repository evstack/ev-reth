# Upgrade Guide: v0.4.0

This guide covers the configuration changes required to upgrade ev-reth to v0.4.0. For a full list of changes, see the [CHANGELOG](../CHANGELOG.md).

> **Note:** v0.3.0 was never released. Operators running a v0.3.0-beta build should follow "Upgrading from v0.2.x" below.

## Upgrading from v0.2.x

No configuration changes required. Rebuild and deploy the new binary.

**What changes automatically:**

- Reth v2.0.0 engine (same config, new internals)
- New nodes use Storage V2 by default. Existing V1 data directories continue working as-is
- Txpool fallback (pulling pending transactions when Engine API attributes are empty) is now only enabled in `--dev` mode
- EIP-2718 payload decode fix takes effect immediately

**Build system:** If you have scripts referencing `make`, update them to use `just`.

**Storage V2 notes:**

- V1 and V2 are not interchangeable. Once a node starts with V2, it cannot go back
- No automatic migration. Switching to V2 requires a full resync
- V1 is deprecated upstream. Plan your migration before support is removed
- If using MDBX backup scripts (e.g. `mdbx_copy`), V2 nodes also use RocksDB for indices, so backup tooling may need updating

**Custom code:** If you import from `reth-primitives`, update imports to `alloy_consensus` or `reth_ethereum_primitives` (the crate was removed upstream).

### Osaka / EOF hardfork

The Osaka hardfork (EVM Object Format, EOFv1) is available but not activated by default. If your chainspec does not already set `osakaTime`, the chain stays on Cancun rules and no action is required.

To schedule activation, add `osakaTime` to your chainspec `config` section with a future Unix timestamp (use `0` on new testnets to activate from genesis):

```json
{
  "config": {
    "osakaTime": 1893456000
  }
}
```

Osaka introduces EOFv1 contracts and related EIPs. See the [v0.2.0 upgrade guide](UPGRADE-v0.2.0.md) for the original `osakaTime` rollout notes and the [Ethereum EOF meta EIP (EIP-7692)](https://eips.ethereum.org/EIPS/eip-7692) for the full list of included changes.

## Upgrading from v0.1.x

First follow [UPGRADE-v0.2.0.md](UPGRADE-v0.2.0.md) and [UPGRADE-v0.2.2.md](UPGRADE-v0.2.2.md) to reach v0.2.x, then apply "Upgrading from v0.2.x" above. Those guides cover the required `osakaTime`, base fee redirect, native token minting precompile, contract size limit, deploy allowlist, and EIP-1559 parameter chainspec changes.

## Complete Chainspec Reference

All `config.evolve` fields available in v0.4.0:

| Field | Type | Default | Since | Description |
|-------|------|---------|-------|-------------|
| `baseFeeSink` | `address` | -- | v0.2.0 | Receives redirected base fees |
| `baseFeeRedirectActivationHeight` | `u64` | `0` | v0.2.0 | Block height when redirect activates |
| `mintAdmin` | `address` | -- | v0.2.0 | Admin for mint/burn precompile |
| `mintPrecompileActivationHeight` | `u64` | `0` | v0.2.0 | Block height when precompile activates |
| `contractSizeLimit` | `usize` | `24576` | v0.2.0 | Max contract code size in bytes |
| `contractSizeLimitActivationHeight` | `u64` | `0` | v0.2.0 | Block height when custom limit activates |
| `deployAllowlist` | `address[]` | `[]` | v0.2.2 | Addresses allowed to deploy contracts (max 1024) |
| `deployAllowlistActivationHeight` | `u64` | `0` | v0.2.2 | Block height when allowlist activates |
| `baseFeeMaxChangeDenominator` | `u64` | `8` | v0.2.2 | Max base fee change per block |
| `baseFeeElasticityMultiplier` | `u64` | `2` | v0.2.2 | Gas target multiplier |
| `initialBaseFeePerGas` | `u64` | `1000000000` | v0.2.2 | Initial base fee in wei |

Top-level `config` fields:

| Field | Type | Default | Since | Description |
|-------|------|---------|-------|-------------|
| `osakaTime` | `u64` | -- | v0.2.0 | Unix timestamp to activate Osaka/EOF hardfork |

## Complete Chainspec Example

```json
{
  "config": {
    "chainId": 12345,
    "homesteadBlock": 0,
    "eip150Block": 0,
    "eip155Block": 0,
    "eip158Block": 0,
    "byzantiumBlock": 0,
    "constantinopleBlock": 0,
    "petersburgBlock": 0,
    "istanbulBlock": 0,
    "berlinBlock": 0,
    "londonBlock": 0,
    "parisBlock": 0,
    "shanghaiTime": 0,
    "cancunTime": 0,
    "osakaTime": 1893456000,
    "terminalTotalDifficulty": 0,
    "terminalTotalDifficultyPassed": true,
    "evolve": {
      "baseFeeSink": "0x00000000000000000000000000000000000000fe",
      "baseFeeRedirectActivationHeight": 0,
      "baseFeeMaxChangeDenominator": 5000,
      "baseFeeElasticityMultiplier": 10,
      "initialBaseFeePerGas": 100000000000000000,
      "mintAdmin": "0x000000000000000000000000000000000000Ad00",
      "mintPrecompileActivationHeight": 0,
      "contractSizeLimit": 131072,
      "contractSizeLimitActivationHeight": 0,
      "deployAllowlist": [
        "0xYourDeployerAddress"
      ],
      "deployAllowlistActivationHeight": 0
    }
  },
  "difficulty": "0x1",
  "gasLimit": "0x2faf080",
  "baseFeePerGas": "0x16345785d8a0000",
  "alloc": {}
}
```

## Related Documentation

- [EIP-1559 Configuration](eip1559-configuration.md) -- tuning base fee parameters
- [Permissioned EVM Guide](guide/permissioned-evm.md) -- deploy allowlist details
- [Fee System Guide](guide/fee-systems.md) -- base fee redirect and FeeVault
- [ADR 003: Typed Transactions](adr/ADR-0003-typed-transactions-sponsorship.md) -- EvNode 0x76 spec

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>
