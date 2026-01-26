# Upgrade Guide: v0.2.2

This guide covers the new features and configuration changes in ev-reth v0.2.2.

## New Features

### Permissioned EVM: Deploy Allowlist

v0.2.2 introduces a deploy allowlist that restricts top-level contract creation to a set of approved EOAs. This is useful for permissioned chains where only authorized deployers should create contracts.

**Key characteristics:**

- Only top-level contract creation transactions are checked
- Contract-to-contract `CREATE/CREATE2` is still allowed
- An empty or missing allowlist means unrestricted deployment (disabled)
- Maximum 1024 addresses allowed

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

| Field                             | Type        | Description                                                        |
|-----------------------------------|-------------|--------------------------------------------------------------------|
| `deployAllowlist`                 | `address[]` | List of EOAs allowed to deploy contracts                           |
| `deployAllowlistActivationHeight` | `number`    | Block height at which the allowlist becomes active (defaults to 0) |

**Important:** This does not create a fully permissioned chain. Non-allowlisted EOAs can still deploy via existing factory contracts if those factories allow it.

See [Permissioned EVM Guide](guide/permissioned-evm.md) for full details.

### EIP-1559 Configuration

v0.2.2 adds support for customizing EIP-1559 base fee parameters in the chainspec. This allows tuning fee market behavior for your specific use case.

**Chainspec configuration** (inside `config.evolve`):

```json
"evolve": {
  "baseFeeMaxChangeDenominator": 8,
  "baseFeeElasticityMultiplier": 2,
  "initialBaseFeePerGas": 1000000000
}
```

| Field                         | Type     | Default    | Description                                                      |
|-------------------------------|----------|------------|------------------------------------------------------------------|
| `baseFeeMaxChangeDenominator` | `number` | 8          | Controls max base fee change per block (higher = slower changes) |
| `baseFeeElasticityMultiplier` | `number` | 2          | Gas target multiplier for elasticity                             |
| `initialBaseFeePerGas`        | `number` | 1000000000 | Initial base fee in wei (1 gwei default)                         |

All fields are optional and default to Ethereum mainnet values if omitted. Existing networks upgrading to v0.2.2 do not need to add these fields - behavior is unchanged unless explicitly configured.

**Warning:** These parameters apply from genesis with no activation height. Only configure for new networks. Changing on an existing network would invalidate historical block validation.

### AdminProxy Contract

v0.2.2 introduces the AdminProxy contract to solve the bootstrap problem for admin addresses at genesis. It allows deploying an admin proxy at genesis with a known owner, then transferring ownership to a multisig post-genesis.

**Use cases:**

- Pre-deploy an admin contract when the final multisig address is unknown at genesis
- Manage mint precompile allowlists
- Manage FeeVault configuration

**Genesis deployment:**

```json
{
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x<ADMIN_PROXY_BYTECODE>",
      "storage": {
        "0x0": "0x000000000000000000000000<OWNER_ADDRESS_WITHOUT_0x>"
      }
    }
  }
}
```

Generate the alloc entry using the helper script:

```bash
cd contracts
OWNER=0xYourEOAAddress forge script script/GenerateAdminProxyAlloc.s.sol -vvv
```

See [AdminProxy Documentation](contracts/admin_proxy.md) for full details including ownership transfer and usage examples.

## Complete Chainspec Example

Here's a complete chainspec for v0.2.2 with all new configuration options:

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
      "mintAdmin": "0x000000000000000000000000000000000000Ad00",
      "mintPrecompileActivationHeight": 0,
      "contractSizeLimit": 131072,
      "contractSizeLimitActivationHeight": 0,
      "deployAllowlist": [
        "0xYourDeployerAddress"
      ],
      "deployAllowlistActivationHeight": 0,
      "baseFeeMaxChangeDenominator": 8,
      "baseFeeElasticityMultiplier": 2,
      "initialBaseFeePerGas": 1000000000
    }
  },
  "difficulty": "0x1",
  "gasLimit": "0x1c9c380",
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x<ADMIN_PROXY_BYTECODE>",
      "storage": {
        "0x0": "0x000000000000000000000000<OWNER_ADDRESS>"
      }
    }
  }
}
```

## Upgrade for Existing Networks

For networks already running v0.2.1, use activation heights to safely introduce the deploy allowlist:

```json
"evolve": {
  "deployAllowlist": [
    "0xYourDeployerAddress"
  ],
  "deployAllowlistActivationHeight": 25000000
}
```

**Note:** EIP-1559 parameter changes and AdminProxy deployment require genesis modification and cannot be safely introduced to existing networks without a coordinated hardfork.

## Migration Checklist

- [ ] Review new configuration options and decide which to enable
- [ ] If using deploy allowlist: add `deployAllowlist` and `deployAllowlistActivationHeight`
- [ ] If customizing EIP-1559: add base fee parameters (new networks only)
- [ ] If using AdminProxy: generate and add alloc entry to genesis (new networks only)
- [ ] Test chainspec changes on a local/testnet deployment
- [ ] Coordinate upgrade timing with network validators/operators
- [ ] Deploy new ev-reth binary
- [ ] Verify node starts and syncs correctly

## Related Documentation

- [Permissioned EVM Guide](guide/permissioned-evm.md)
- [AdminProxy Documentation](contracts/admin_proxy.md)
- [Fee System Guide](guide/fee-system.md)
- [ADR 003: Typed Sponsorship Transactions](adr/ADR-0003-typed-sponsorship-transactions.md)

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>
