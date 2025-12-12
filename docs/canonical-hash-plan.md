# Plan: Restore Canonical Block Hashes While Preserving Rollkit Apphash

## Goals

- Emit keccak-based canonical hashes inside every header so upstream Reth tooling no longer reports continuous forks.
- Preserve the Rollkit `apphash` in a discoverable location so DA clients continue to verify block linkage.
- Gate the change behind a chainspec activation height for deterministic rollouts.

## 1. Payload Builder & Types

- Update `EvolvePayloadBuilder` (`crates/node/src/builder.rs`) to compute the keccak hash after `builder.finish` and assign it to `header.hash`/`parent_hash` before sealing.
- Extend `EvolveEnginePayloadAttributes` and related types (`crates/evolve/src/types.rs`, `crates/node/src/attributes.rs`) to carry the incoming `apphash` separately from the canonical hash.
- Persist the `apphash` to a new header field (candidate: encode in `extra_data` or define a dedicated header extension struct shared across payload serialization).
- Introduce activation-aware logic: pre-activation blocks keep legacy behavior; post-activation blocks always write canonical hashes while still storing the `apphash` in the new location.

## 2. Validator & Consensus

- Modify `EvolveEngineValidator` (`crates/node/src/validator.rs`) to stop bypassing `PayloadError::BlockHashMismatch` after the activation height. Retain the bypass for legacy blocks.
- Ensure `EvolveConsensus` (`crates/evolve/src/consensus.rs`) now validates hash/parentHash linkages post-activation while keeping the relaxed timestamp rule.
- Audit any code paths that rely on the `apphash` (e.g., Rollkit-specific checks) and point them to the relocated field.

## 3. RPC & Serialization

- Adjust the conversion helpers that produce `ExecutionPayload` / `ExecutionPayloadEnvelope` values so RPC consumers see the canonical hash in the standard field and the `apphash` via either `extraData` or a new optional field.
- Clearly document the new field semantics so explorers/light clients know where to read the Rollkit hash.
- Maintain backward compatibility in request parsing: continue accepting payload attributes that include the `apphash`, even though it is no longer stored in `header.hash`.

## 4. Chainspec & Configuration

- Add a new evolve config flag, e.g. `hashRewireActivationHeight`, parsed in `crates/evolve/src/config.rs` and surfaced through `EvolvePayloadBuilderConfig`.
- Validate the flag alongside existing evolve settings; log or reject invalid configurations.
- Update sample configs (`etc/ev-reth-genesis.json`) and the README/upgrade docs with instructions for setting the activation height.

## 5. Testing & Tooling

- Extend e2e tests (`crates/tests/src/e2e_tests.rs`, `test_evolve_engine_api.rs`) to cover both pre- and post-activation behavior, asserting that:
  - Legacy mode still bypasses hash mismatches.
  - Post-activation blocks produce canonical parent links and expose the `apphash` in the new field.
- Add unit tests around the serialization helpers to ensure RPC payloads echo both hashes correctly.
- Verify Rollkit integration tests continue to pass once they read the `apphash` from the new location.

## 6. Rollout Steps

1. Implement the code changes behind the activation height flag and land them with comprehensive tests.
2. Publish an upgrade note describing the new flag, how to set the activation block, and how to verify the behavior via RPC.
3. Coordinate with testnet/mainnet operators to schedule the activation block and ensure explorers/monitoring tools understand the relocated `apphash` field.
4. After activation, monitor forkchoice health and Rollkit ingestion to confirm both canonical and DA workflows function correctly.
