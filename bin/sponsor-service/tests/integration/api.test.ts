import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import type { EvNodeTransaction, SponsorableIntent } from '@evstack/evnode-viem';
import { signAsExecutor, encodeSignedTransaction } from '@evstack/evnode-viem';
import { serializeIntent } from '../../src/serialization.js';
import { createServer } from '../../src/server.js';
import type { FastifyInstance } from 'fastify';
import { makeConfig, makeHashSigner, makeIntent, makeTx } from '../helpers/fixtures.js';

describe('REST API', () => {
  let app: FastifyInstance;

  beforeEach(() => {
    const server = createServer(makeConfig());
    app = server.app;
  });

  afterEach(async () => {
    await app.close();
  });

  test('GET /v1/health returns status', async () => {
    const response = await app.inject({ method: 'GET', url: '/v1/health' });
    expect(response.statusCode).toBe(200);
    const body = response.json();
    expect(body).toHaveProperty('status');
    expect(body).toHaveProperty('nodeConnected');
  });

  test('GET /v1/policy returns config', async () => {
    const response = await app.inject({ method: 'GET', url: '/v1/policy' });
    expect(response.statusCode).toBe(200);
    const body = response.json();
    expect(body.chainId).toBe('1337');
    expect(body).toHaveProperty('sponsorAddress');
    expect(body).toHaveProperty('maxGasPerTx');
  });

  test('POST /v1/sponsor rejects invalid chain ID', async () => {
    const intent = await makeIntent();
    const serialized = serializeIntent(intent);
    serialized.tx.chainId = '0x3e7'; // 999

    const response = await app.inject({
      method: 'POST',
      url: '/v1/sponsor',
      payload: serialized,
    });
    expect(response.statusCode).toBeGreaterThanOrEqual(400);
  });

  test('POST /v1/sponsor rejects gas limit exceeded', async () => {
    const intent = await makeIntent(makeTx({ gasLimit: 1_000_000n }));

    const response = await app.inject({
      method: 'POST',
      url: '/v1/sponsor',
      payload: serializeIntent(intent),
    });
    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.error).toBe('GAS_LIMIT_EXCEEDED');
  });
});

describe('JSON-RPC proxy', () => {
  let app: FastifyInstance;

  beforeEach(() => {
    const server = createServer(makeConfig());
    app = server.app;
  });

  afterEach(async () => {
    await app.close();
  });

  test('eth_sendRawTransaction with 0x76 tx without sponsor sig triggers sponsoring', async () => {
    const intent = await makeIntent();
    const rawTx = encodeSignedTransaction({
      transaction: intent.tx,
      executorSignature: intent.executorSignature,
    });

    const response = await app.inject({
      method: 'POST',
      url: '/',
      payload: {
        jsonrpc: '2.0',
        id: 1,
        method: 'eth_sendRawTransaction',
        params: [rawTx],
      },
    });

    const body = response.json();
    // Will fail at balance check (no real node), but should get past decode + policy
    // The error proves the service decoded the tx and tried to sponsor it
    expect(body.jsonrpc).toBe('2.0');
    expect(body.id).toBe(1);
    expect(body.error).toBeDefined();
    // Should fail at sponsor balance check or node connection, not at decoding
    expect(body.error.message).toMatch(/balance|Node error|connect|fetch/i);
  });

  test('eth_sendRawTransaction with 0x76 tx rejects gas limit exceeded', async () => {
    const signer = makeHashSigner();
    const tx = makeTx({ gasLimit: 1_000_000n });
    const executorSignature = await signAsExecutor(tx, signer);
    const rawTx = encodeSignedTransaction({
      transaction: tx,
      executorSignature,
    });

    const response = await app.inject({
      method: 'POST',
      url: '/',
      payload: {
        jsonrpc: '2.0',
        id: 1,
        method: 'eth_sendRawTransaction',
        params: [rawTx],
      },
    });

    const body = response.json();
    expect(body.jsonrpc).toBe('2.0');
    expect(body.error).toBeDefined();
    expect(body.error.code).toBe(-32602);
    expect(body.error.message).toMatch(/Gas limit/);
  });

});
