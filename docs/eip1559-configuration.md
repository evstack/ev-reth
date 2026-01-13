# EIP-1559 Configuration for 100ms Blocks

## TLDR

Recommendations for gas limit, base fee, and EIP-1559 parameters
for mainnet launch with 100ms blocks.

## Recommended Configuration

| Parameter | Recommendation | Rationale |
| --- | --- | --- |
| **Gas Limit** | 50M per block | Balances throughput (500M gas/sec) with execution time constraints |
| **Initial Base Fee** | 0.1 ntia | Provides spam protection |
| **Minimum Base Fee** | 0.1 ntia | Prevents drift to zero, ensures sustainable economics |
| **EIP1559 Denominator** | 5000 | Smooth fee adjustments: +/-0.02%/block, +/-0.2% per second, same as OP networks |
| **EIP1559 Elasticity Multiplier** | 10 | Lower per block target for eip1559 while allowing 10x burst capacity |

## Why These Numbers

### Gas Limit: 50M per block

- **Throughput:** 500M gas/second theoretical max
- Start here, can scale up later if needed

### Base Fee: 0.1 ntia minimum/initial

- **DA coverage:** Should ensure gas fees cover data availability costs, but will have to run analysis post-launch
- **Spam protection:** Makes attacks expensive while keeping normal use cheap
- **Testnet lesson:** Without a minimum, testnet dropped to 7 atia (wei)

### Denominator: 5000

**Problem with default:** +/-12.5% per block = extreme volatility at 100ms

- 5 full blocks (0.5 sec) is a 61% fee increase
- Unpredictable costs, wallet estimations out of range between simulation and submission, poor UX

**With 5000:**

- +/-0.02% per block; +/-0.2% per second; +/-2.4% per 12 seconds
- Smoother changes over time
- Similar to Optimism's adjustment rate

### Elasticity: 10

**Problem with default:** Target would be 25M gas/block (50% of max)

- Requires 250M gas/second sustained to maintain base fee
- Unrealistic with bursty 100ms traffic
- Causes constant downward pressure; fees drift to zero

## Comparison to Other Chains

| Chain | Block Time | Block Gas Limit | Denominator | Elasticity |
| --- | --- | --- | --- | --- |
| Ethereum | 12s | 30M | 8 | 2 |
| Base | 2s | 300M | 250 | 6 |
| **Eden (proposed)** | 100ms | 50M | 5000 | 10 |

## Configuration Mapping (ev-reth)

The example chainspec at `etc/ev-reth-genesis.json` already uses the recommended defaults.

```json
{
  "gasLimit": "0x2faf080",
  "baseFeePerGas": "0x16345785d8a0000",
  "config": {
    "londonBlock": 0,
    "evolve": {
      "baseFeeMaxChangeDenominator": 5000,
      "baseFeeElasticityMultiplier": 10,
      "initialBaseFeePerGas": 100000000000000000
    }
  }
}
```

Notes:

- `baseFeePerGas` is the genesis base fee; `initialBaseFeePerGas` sets the same value when
  `londonBlock` is `0`. Keep them consistent.
- The values above assume 18 decimals for `ntia` (0.1 ntia = 100000000000000000).
- ev-reth does not enforce a protocol-level minimum base fee. If you need a floor, use the
  txpool admission guard (e.g., `--txpool.minimal-protocol-basefee`) and align wallet defaults.
