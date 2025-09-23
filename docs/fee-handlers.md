# Fee Handler Chainspec Extension

Evolve networks redirect execution-layer fees by embedding a `feeHandlers` block in the chainspec. The block lives under `config.ev_reth.feeHandlers` inside the genesis JSON and is loaded by every node at startup. The same values **must** be distributed to all participants on a given network: diverging configs change the block state root and will fork the chain.

The full structure looks like this:

```json
{
  "config": {
    "… standard merge/london fields …": "…",
    "ev_reth": {
      "feeHandlers": {
        "vaults": {
          "sequencer_fee_vault": "0x…",
          "base_fee_vault": "0x…",
          "l1_fee_vault": "0x…",
          "operator_fee_vault": "0x…" // optional
        },
        "l1_params": {
          "mode": "V1",
          "v1": {
            "share_size": 512,
            "overhead_shares": 1,
            "blob_price_scalar": 1000000,
            "decimals": 6
          }
        },
        "operator_fee": {
          "constant": 0,
          "scalar": 0,
          "enabled": false
        }
      }
    }
  },
  "difficulty": "0x1",
  "gasLimit": "0x1c9c380",
  "alloc": {}
}
```

## Fields

### `vaults`

| Field | Type | Description |
|-------|------|-------------|
| `sequencer_fee_vault` | `address` | Account that receives the block coinbase. This is what the payload builder sets as `suggested_fee_recipient`. |
| `base_fee_vault` | `address` | Account credited with the total EIP‑1559 base fee (`base_fee_per_gas × gas_used`). |
| `l1_fee_vault` | `address` | Account credited with the data-availability (L1) reimbursement computed by `l1_params`. |
| `operator_fee_vault` | `address` or `null` | Optional vault for operator revenue. Omit or set to `null` if unused. |

Every address must be a 20-byte hex string with `0x` prefix. Set an address to the zero address (`0x000…000`) if you want to burn that bucket.

### `l1_params`

`mode` selects the fee model. The current implementation supports the `V1` Celestia shares model:

- `share_size` (bytes): effective size of a DA share (Celestia uses 512).
- `overhead_shares`: constant per-block share overhead added after rounding.
- `blob_price_scalar`: numerator used to scale L1 blob price into wei.
- `decimals`: decimal shift applied to the scalar (fee = shares × scalar × price / 10^decimals).

The runtime multiplies the total number of shares by the (optional) `EV_RETH_CELESTIA_BLOB_BASE_FEE_WEI` environment variable for the current block. Leave the variable unset to treat the L1 blob price as zero.

### `operator_fee`

Provides an optional additional fee component:

- `enabled`: when `true`, include the operator fee; when `false`, the other fields are ignored.
- `constant` (wei): fixed amount credited every block when enabled.
- `scalar` (wei): per-gas multiplier. The fee builder computes `scalar × gas_used / 1_000_000` (the division matches Optimism’s operator fee convention).

## Disabling or Changing Fees

- **Burn everything:** remove the entire `ev_reth.feeHandlers` block (or distribute a chainspec without it). The payload builder detects the absence and reverts to the legacy burn-only path.
- **Change vaults or parameters:** publish an updated chainspec with the modified JSON and ensure all nodes adopt it before the activation block/slot.

## Checklist for a New Network

1. Copy `etc/ev-reth-genesis.json` as a starting point.
2. Edit the `vaults` addresses to match your deployment.
3. Tweak `l1_params` to reflect your DA platform (share size, overhead, pricing formula).
4. Decide whether to enable `operator_fee`.
5. Distribute the chainspec together with the genesis alloc and bootstrap instructions.
6. (Optional) Configure `EV_RETH_CELESTIA_BLOB_BASE_FEE_WEI` in your node environment until an automated fee oracle feeds blob prices.

Keep the chainspec under version control so future upgrades (new fee modes, different vaults) can be rolled out and audited easily.
