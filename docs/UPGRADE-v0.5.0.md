# Upgrade Guide: v0.5.0

This guide covers the configuration changes required to upgrade ev-reth to v0.5.0. For a full list of changes, see the [CHANGELOG](../CHANGELOG.md).

## Upgrading from v0.4.x

No chainspec changes are required for existing networks. Rebuild and deploy the new binary. Existing chains keep current proposer behavior until `proposerControlAdmin` is set.

Reth is now v2.5.0 (from v2.2.0 in v0.4.1). That is an internal engine bump: no new required `config` fields.

### Removed tooling

`ev-deployer` and `ev-dev` were removed. If you used them:

- Generate genesis allocs with the Foundry scripts under `contracts/script/` (`GenerateFeeVaultAlloc`, `GenerateAdminProxyAlloc`)
- Run a local stack with the `ev-reth` binary plus ev-node, not `ev-dev`

## New Features

### Proposer Control Precompile

v0.5.0 adds an optional precompile at `0x000000000000000000000000000000000000F101` that stores the next ev-node proposer in execution state. Rotation is a normal transaction from the configured admin.

The feature is off unless `proposerControlAdmin` is set. When it is configured, ev-reth also registers `evolve_getNextProposer`.

**Chainspec configuration** (inside `config.evolve`):

```json
"evolve": {
  "proposerControlAdmin": "0x000000000000000000000000000000000000Ad00",
  "proposerControlActivationHeight": 0,
  "initialNextProposer": "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd"
}
```

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `proposerControlAdmin` | `address` | -- | Enables the precompile and authorizes `setNextProposer`. Zero or omitted leaves the feature disabled. Use [AdminProxy](contracts/admin_proxy.md) in production; an EOA cannot be rotated without a hard fork. |
| `proposerControlActivationHeight` | `u64` | `0` | Block height when the precompile becomes callable. Defaults to `0` when the admin is set. |
| `initialNextProposer` | `bytes32` | zero | Value returned until the first rotation. Left-pad a 20-byte address. If omitted, reads return zero and ev-node keeps the genesis proposer. |

**New chains:** set `proposerControlActivationHeight` to `0` and `initialNextProposer` to the genesis sequencer.

**Existing chains:** pick a future activation height, upgrade every full node and the sequencer first, and set `initialNextProposer` to the currently expected signer.

Before the activation height, a call to `0xF101` is an empty-account call: it succeeds and writes nothing. The precompile does not emit logs; confirm with `nextProposer()` or `evolve_getNextProposer`. A garbage `bytes32` is not validated and can stall the chain.

See [ADR 0004](adr/ADR-0004-proposer-rotation-precompile.md) and [AdminProxy](contracts/admin_proxy.md) for the rotation transaction.

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
      "proposerControlAdmin": "0x000000000000000000000000000000000000Ad00",
      "proposerControlActivationHeight": 0,
      "initialNextProposer": "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd",
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

## Migration Checklist

- [ ] Deploy the v0.5.0 binary to every full node, then the sequencer
- [ ] If enabling proposer control on a new chain: set admin, activation `0`, and `initialNextProposer`
- [ ] If enabling on an existing chain: set a future `proposerControlActivationHeight` and `initialNextProposer` to the current sequencer
- [ ] Point `proposerControlAdmin` at AdminProxy (or another governance contract), not an EOA
- [ ] After activation, rotate with `AdminProxy.execute(0xF101, setNextProposer(newProposer))`
- [ ] Confirm via `evolve_getNextProposer` and start the new sequencer before it must produce a block

## Related Documentation

- [ADR 0004: Proposer Rotation Precompile](adr/ADR-0004-proposer-rotation-precompile.md)
- [AdminProxy](contracts/admin_proxy.md)
- [v0.4.0 upgrade guide](UPGRADE-v0.4.0.md)

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>
