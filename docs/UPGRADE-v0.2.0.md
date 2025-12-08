# Upgrade Guide: v0.2.0

This guide covers the required changes to upgrade ev-reth from v0.1.x to v0.2.0.

## Breaking Changes

### Fusaka Upgrade: `osakaTime` Configuration Required

v0.2.0 adds support for the Fusaka hard fork (based on Ethereum's Osaka upgrade). **You must set `osakaTime` in your chainspec to a future timestamp** to prevent the upgrade from activating immediately.

If `osakaTime` is not set or is set to `0`, the Osaka fork will activate at genesis, which may cause unexpected behavior on existing networks.

#### Action Required

Add `osakaTime` to your chainspec's `config` section with a future Unix timestamp:

```json
{
  "config": {
    "chainId": 1,
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
      ...
    }
  }
}
```

**Important:** Choose a timestamp far enough in the future to allow for coordinated network upgrades. The example above (`1893456000`) corresponds to January 1, 2030.

For testnet deployments where you want to test Osaka features immediately, you can set `osakaTime` to `0`.

## New Configuration Options

### Native Token Minting Precompile

v0.2.0 introduces a native token minting precompile that allows authorized addresses to mint and burn the native token. Add these fields to your chainspec's `evolve` section:

| Field | Type | Description |
|-------|------|-------------|
| `mintPrecompileAdmin` | `address` | Admin address that can manage the allowlist and mint/burn tokens |
| `mintPrecompileActivationHeight` | `number` | Block height at which the precompile becomes active |

```json
"evolve": {
  "mintPrecompileAdmin": "0xYourAdminAddressHere",
  "mintPrecompileActivationHeight": 0
}
```

Set `mintPrecompileAdmin` to `0x0000000000000000000000000000000000000000` to disable the minting precompile entirely.

For existing networks, set `mintPrecompileActivationHeight` to a future block to ensure archival nodes remain compatible with historical state.

See [ADR-0002: Native Token Minting Precompile](adr/ADR-0002-native-minting-precompile.md) for full details.

## Complete Chainspec Example

Here's a complete example chainspec for v0.2.0 with all new configuration options:

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
      "mintPrecompileAdmin": "0xYourAdminAddressHere",
      "mintPrecompileActivationHeight": 0,
      "contractSizeLimit": 131072,
      "contractSizeLimitActivationHeight": 0
    }
  },
  "difficulty": "0x1",
  "gasLimit": "0x1c9c380",
  "alloc": {}
}
```

## Upgrade for Existing Networks

For networks already running v0.1.x, use activation heights to safely introduce new features:

```json
"evolve": {
  "baseFeeSink": "0x00000000000000000000000000000000000000fe",
  "baseFeeRedirectActivationHeight": 20000000,
  "mintPrecompileAdmin": "0xYourAdminAddressHere",
  "mintPrecompileActivationHeight": 20000000,
  "contractSizeLimit": 131072,
  "contractSizeLimitActivationHeight": 20000000
}
```

This ensures:

1. Historical blocks remain valid and verifiable
2. Archival nodes can sync from genesis without issues
3. New features activate at a coordinated block height

## Migration Checklist

- [ ] Update chainspec with `osakaTime` set to a future timestamp
- [ ] Add `mintPrecompileAdmin` if native token minting is needed
- [ ] Add `mintPrecompileActivationHeight` (use future block for existing networks)
- [ ] Review all activation heights for existing network compatibility
- [ ] Test chainspec changes on a local/testnet deployment
- [ ] Coordinate upgrade timing with network validators/operators
- [ ] Deploy new ev-reth binary
- [ ] Verify node starts and syncs correctly

## FeeVault Contract Updates

If using the FeeVault contract for base fee collection, the constructor now accepts additional deployment configuration parameters. See [contracts/README.md](../contracts/README.md) for updated deployment instructions.

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>
