# Upgrade Guide: v0.3.0

This guide covers the new features and changes in ev-reth v0.3.0.

## Breaking Changes

### Reth Upgraded to v1.11.0

The underlying Reth dependency has been upgraded from v1.8.4 to v1.11.0. This is a major version bump that includes changes to EVM handler architecture, payload builder interfaces, and execution primitives.

**Action Required:** Rebuild from source. No chainspec changes are needed for this upgrade alone.

### Default Features Disabled for SP1 Compatibility

Several reth crate dependencies now use `default-features = false` to unblock SP1 proving work. The `c-kzg` dependency was also removed from `reth-ethereum-primitives`. This reduces binary size and compilation time for SP1 verification circuits but may affect downstream consumers who relied on default features being enabled.

## New Features

### EvNode Transaction Type (0x76)

v0.3.0 introduces a new EIP-2718 typed transaction (`0x76`) that natively supports **gas sponsorship** and **atomic batch calls**. This is the headline feature of the release.

**Key capabilities:**

- **Batch Calls:** Multiple operations execute atomically within a single transaction. All calls succeed or the entire transaction reverts.
- **Fee-Payer Sponsorship:** An optional sponsor signature allows a separate account to pay gas on behalf of the executor without changing `tx.origin`.
- **Open Sponsorship Model:** The executor signs with an empty sponsor field, allowing any sponsor to pick up the signed intent and pay gas. This enables "Gas Station" style networks.

**Transaction structure:**

```
Type: 0x76
Envelope: 0x76 || rlp([chain_id, nonce, max_priority_fee_per_gas, max_fee_per_gas, gas_limit, calls, access_list, fee_payer_signature, v, r, s])
```

| Field | Type | Description |
|-------|------|-------------|
| `chain_id` | `u64` | Chain identifier |
| `nonce` | `u64` | Executor nonce |
| `max_priority_fee_per_gas` | `u128` | EIP-1559 priority fee |
| `max_fee_per_gas` | `u128` | EIP-1559 max fee |
| `gas_limit` | `u64` | Gas limit for entire batch |
| `calls` | `Vec<Call>` | Batch of operations (to, value, input) |
| `access_list` | `AccessList` | State access hints |
| `fee_payer_signature` | `Option<Signature>` | Optional sponsor authorization |

**Validation rules:**

- At least one call is required
- Only the first call may be a CREATE; subsequent calls must be CALL
- Executor signature must be valid for domain `0x76`
- Sponsor signature (if present) must be valid for domain `0x78`

**Signature domains:**

| Domain | Byte | Signer | Purpose |
|--------|------|--------|---------|
| Executor | `0x76` | Transaction sender | Authorizes the intent |
| Sponsor | `0x78` | Fee payer | Authorizes gas payment for a specific executor intent |

**No chainspec changes required.** The 0x76 transaction type is protocol-level and does not require any configuration. It is available on all networks running v0.3.0.

See [ADR 003](adr/ADR-0003-typed-transactions-sponsorship.md) for the full specification.

### Viem Client Library (`@evstack/evnode-viem`)

A TypeScript/JavaScript client library is now available in `clients/` for creating and managing EvNode transactions using [Viem](https://viem.sh).

**Package:** `@evstack/evnode-viem` (requires `viem ^2.0.0` as peer dependency)

**Supported flows:**

1. **Basic transaction** -- executor pays gas, single or batch calls
2. **Sponsored transaction** -- sponsor pays gas on behalf of executor
3. **Intent-based sponsorship** -- executor signs intent off-chain, sponsor picks it up and signs separately
4. **Contract deployment** -- CREATE call as first operation in a batch

**Example usage:**

```typescript
import { createEvnodeClient } from '@evstack/evnode-viem'

// Create client with executor wallet
const client = createEvnodeClient({
  rpcUrl: 'http://localhost:8545',
  executor: executorAccount,
})

// Send a basic transaction
await client.send({
  calls: [{ to: '0x...', value: 0n, data: '0x...' }],
  gasLimit: 100000n,
  maxFeePerGas: 1000000000n,
  maxPriorityFeePerGas: 1000000n,
})

// Create a sponsorable intent
const intent = await client.createIntent({ calls, gasLimit, maxFeePerGas, maxPriorityFeePerGas })

// Sponsor and send (from sponsor side)
await sponsorClient.sponsorAndSend(intent)
```

**RPC extensions:**

- `eth_getTransactionByHash` responses include a `feePayer` field (address) when the transaction is sponsored
- `eth_getTransactionReceipt` indicates the effective gas payer

### Permissioned EVM: Gas Validation Fix

v0.3.0 fixes deploy allowlist enforcement when gas is explicitly specified. Previously, the deploy allowlist check could be bypassed in certain gas-specified scenarios. The fix ensures:

- Deploy allowlist validation applies uniformly to both standard Ethereum and EvNode transactions
- Transaction pool admission validates deploy permissions upfront to prevent DoS
- For sponsored EvNode transactions, the sponsor's balance is validated against `max_fee_per_gas * gas_limit`

**No chainspec changes required.** This is a correctness fix for the existing `deployAllowlist` feature from v0.2.2.

### Container: Tini Init Process

The Docker images now use [tini](https://github.com/krallin/tini) as PID 1 for proper signal forwarding. This ensures graceful shutdown when running in containerized environments (Kubernetes, Docker Compose).

**No action required.** This is automatic when using the official Docker image.

## Complete Chainspec Example

The chainspec format is unchanged from v0.2.2. Here is a complete example for reference:

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
  "alloc": {}
}
```

## Upgrade for Existing Networks

v0.3.0 is a drop-in replacement for v0.2.2. No chainspec modifications are required.

1. The EvNode transaction type (0x76) is automatically available once the binary is upgraded
2. The permissioned EVM gas fix takes effect immediately
3. Existing configuration (deploy allowlist, EIP-1559 params, mint precompile) continues to work unchanged

## Migration Checklist

- [ ] Review the new EvNode transaction type and decide if your application will use it
- [ ] If using sponsorship: integrate the `@evstack/evnode-viem` client library
- [ ] If running custom Docker images: verify tini is included or use the official image
- [ ] Test the upgrade on a local/testnet deployment
- [ ] Coordinate upgrade timing with network validators/operators
- [ ] Deploy new ev-reth binary
- [ ] Verify node starts and syncs correctly
- [ ] Verify existing transactions and block production continue working

## Related Documentation

- [ADR 003: Typed Transactions for Sponsorship and Batch Calls](adr/ADR-0003-typed-transactions-sponsorship.md)
- [Permissioned EVM Guide](guide/permissioned-evm.md)
- [Fee System Guide](guide/fee-systems.md)
- [AdminProxy Documentation](contracts/admin_proxy.md)

## Questions?

For issues or questions about the upgrade, please open an issue at <https://github.com/evstack/ev-reth/issues>
