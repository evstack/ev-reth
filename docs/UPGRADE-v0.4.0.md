# Upgrade Guide: v0.4.0

This guide covers the configuration changes required to upgrade ev-reth to v0.4.0. For a full list of changes, see the [CHANGELOG](../CHANGELOG.md).

## Upgrading from v0.3.0

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

## Upgrading from v0.2.2

Everything in "Upgrading from v0.3.0" above, plus the following chainspec change:

### Osaka Hardfork (optional)

Add `osakaTime` to your chainspec `config` section to activate the Osaka hardfork (EOF support). Without it, the chain stays on Cancun rules.

```json
{
  "config": {
    "osakaTime": 1893456000
  }
}
```

Choose a timestamp far enough in the future to coordinate the upgrade across all nodes. Set to `0` on testnets to activate immediately.

No other configuration changes are required. The EvNode transaction type (0x76) is available automatically once the binary is upgraded.

## Upgrading from v0.2.0

Everything in "Upgrading from v0.2.2" above, plus the following chainspec changes inside `config.evolve`:

### Deploy Allowlist (optional)

Restrict top-level contract creation to approved addresses.

```json
{
  "config": {
    "evolve": {
      "deployAllowlist": ["0xYourDeployerAddress"],
      "deployAllowlistActivationHeight": 0
    }
  }
}
```

For existing networks, set `deployAllowlistActivationHeight` to a future block height.

### EIP-1559 Parameters (optional, new networks only)

Customize base fee behavior. These apply from genesis with no activation height, so only configure for new networks.

```json
{
  "config": {
    "evolve": {
      "baseFeeMaxChangeDenominator": 5000,
      "baseFeeElasticityMultiplier": 10,
      "initialBaseFeePerGas": 100000000000000000
    }
  }
}
```

| Field | Default | Description |
|-------|---------|-------------|
| `baseFeeMaxChangeDenominator` | `8` | Max base fee change per block. Higher = slower changes |
| `baseFeeElasticityMultiplier` | `2` | Gas target multiplier |
| `initialBaseFeePerGas` | `1000000000` | Initial base fee in wei |

See [EIP-1559 Configuration](eip1559-configuration.md) for tuning recommendations.

## Upgrading from v0.1.x

Everything in "Upgrading from v0.2.0" above, plus the following chainspec changes:

### Osaka Timestamp (required)

You **must** set `osakaTime` to a future timestamp. If omitted or set to `0`, the Osaka fork activates at genesis, which may cause unexpected behavior on existing networks.

### Base Fee Redirect

Redirect burned base fees to a sink address instead of burning them.

```json
{
  "config": {
    "evolve": {
      "baseFeeSink": "0x00000000000000000000000000000000000000fe",
      "baseFeeRedirectActivationHeight": 0
    }
  }
}
```

For existing networks, set `baseFeeRedirectActivationHeight` to a future block height.

### Native Token Minting Precompile

Enable minting and burning of the native token by authorized addresses.

```json
{
  "config": {
    "evolve": {
      "mintAdmin": "0x000000000000000000000000000000000000Ad00",
      "mintPrecompileActivationHeight": 0
    }
  }
}
```

Set `mintAdmin` to the zero address to disable. For existing networks, set `mintPrecompileActivationHeight` to a future block height.

### Contract Size Limit

Override the default 24KB EIP-170 contract size limit.

```json
{
  "config": {
    "evolve": {
      "contractSizeLimit": 131072,
      "contractSizeLimitActivationHeight": 0
    }
  }
}
```

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
| `osakaTime` | `u64` | -- | v0.3.0 | Unix timestamp to activate Osaka/EOF hardfork |

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
