import { describe, test, expect, beforeEach } from 'bun:test';
import { PolicyEngine } from '../../src/services/policy-engine.js';
import type { SponsorConfig } from '../../src/config.js';
import { makeConfig, makeIntent, makeTx } from '../helpers/fixtures.js';

describe('PolicyEngine', () => {
  let config: SponsorConfig;
  let engine: PolicyEngine;

  beforeEach(() => {
    config = makeConfig();
    engine = new PolicyEngine(config);
  });

  test('accepts valid intent', async () => {
    const intent = await makeIntent();
    await expect(engine.validate(intent)).resolves.toBeUndefined();
  });

  test('rejects wrong chain ID', async () => {
    const tx = makeTx({ chainId: 999n });
    const intent = await makeIntent(tx);
    await expect(engine.validate(intent)).rejects.toThrow('Chain ID mismatch');
  });

  test('rejects invalid executor signature', async () => {
    const intent = await makeIntent();
    const tampered = {
      ...intent,
      executorAddress: '0x70997970C51812dc3A010C7d01b50e0d17dc79C8' as const,
    };
    await expect(engine.validate(tampered)).rejects.toThrow('Executor signature');
  });

  test('rejects gas limit exceeding cap', async () => {
    const tx = makeTx({ gasLimit: 1_000_000n });
    const intent = await makeIntent(tx);
    await expect(engine.validate(intent)).rejects.toThrow('Gas limit');
  });

  test('rejects fee exceeding cap', async () => {
    const tx = makeTx({ maxFeePerGas: 200_000_000_000n });
    const intent = await makeIntent(tx);
    await expect(engine.validate(intent)).rejects.toThrow('Max fee per gas');
  });
});
