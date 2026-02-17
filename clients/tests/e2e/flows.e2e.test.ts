import { describe, it, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createClient, hexToBigInt, http, type Hex, toHex } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { randomBytes } from 'node:crypto';
import { createEvnodeClient, type Call } from '../../src/index.ts';
import { setupTestNode, type TestContext } from './setup.ts';

// Hardhat account #0 — pre-funded in genesis.json
const EXECUTOR_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80' as const;
const TRANSFER_AMOUNT = BigInt('1000000000000000'); // 0.001 ETH
const SPONSOR_FUND_WEI = BigInt('10000000000000000'); // 0.01 ETH
const RECEIPT_TIMEOUT_MS = 30_000;

describe('flows e2e', { timeout: 120_000 }, () => {
  let ctx: TestContext;
  let rpcUrl: string;

  before(async () => {
    ctx = await setupTestNode();
    rpcUrl = ctx.rpcUrl;
  });

  after(async () => {
    if (ctx) await ctx.cleanup();
  });

  function makeClient() {
    const client = createClient({ transport: http(rpcUrl) });
    const executorAccount = privateKeyToAccount(EXECUTOR_KEY);
    const executor = {
      address: executorAccount.address,
      signHash: async (hash: Hex) => sign({ hash, privateKey: EXECUTOR_KEY }),
    };
    return { client, executorAccount, executor };
  }

  it('unsponsored single call', async () => {
    const { client, executorAccount, executor } = makeClient();
    const evnode = createEvnodeClient({ client, executor });

    const balanceBefore = await getBalance(client, executorAccount.address);

    const hash = await evnode.send({
      calls: [{ to: executorAccount.address, value: 0n, data: '0x' }],
    });

    const receipt = await waitForReceipt(client, hash, RECEIPT_TIMEOUT_MS);
    assert.equal(receipt.status, '0x1', 'tx should succeed');

    const balanceAfter = await getBalance(client, executorAccount.address);
    assert.ok(balanceBefore > balanceAfter, 'executor should have spent gas');
  });

  it('unsponsored batch (two transfers)', async () => {
    const { client, executorAccount, executor } = makeClient();
    const evnode = createEvnodeClient({ client, executor });

    const recipient1 = privateKeyToAccount(toHex(randomBytes(32)) as `0x${string}`).address;
    const recipient2 = privateKeyToAccount(toHex(randomBytes(32)) as `0x${string}`).address;

    const recipient1Before = await getBalance(client, recipient1);
    const recipient2Before = await getBalance(client, recipient2);

    const hash = await evnode.send({
      calls: [
        { to: recipient1, value: TRANSFER_AMOUNT, data: '0x' },
        { to: recipient2, value: TRANSFER_AMOUNT, data: '0x' },
      ],
    });

    const receipt = await waitForReceipt(client, hash, RECEIPT_TIMEOUT_MS);
    assert.equal(receipt.status, '0x1', 'batch tx should succeed');

    const recipient1After = await getBalance(client, recipient1);
    const recipient2After = await getBalance(client, recipient2);
    assert.equal(recipient1After - recipient1Before, TRANSFER_AMOUNT, 'recipient1 should receive exact amount');
    assert.equal(recipient2After - recipient2Before, TRANSFER_AMOUNT, 'recipient2 should receive exact amount');
  });

  it('sponsored single call', async () => {
    await runSponsoredTest([{ to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' }]);
  });

  it('sponsored batch', async () => {
    await runSponsoredTest([
      { to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' },
      { to: '0x0000000000000000000000000000000000000002', value: 0n, data: '0x' },
    ]);
  });

  async function runSponsoredTest(calls: Call[]) {
    const { client, executorAccount, executor } = makeClient();

    // Create fresh sponsor
    const sponsorKey = toHex(randomBytes(32)) as `0x${string}`;
    const sponsorAccount = privateKeyToAccount(sponsorKey);
    const sponsor = {
      address: sponsorAccount.address,
      signHash: async (hash: Hex) => sign({ hash, privateKey: sponsorKey }),
    };

    const evnodeUnsponsored = createEvnodeClient({ client, executor });
    const evnodeSponsored = createEvnodeClient({ client, executor, sponsor });

    // Fund the sponsor
    const fundHash = await evnodeUnsponsored.send({
      calls: [{ to: sponsorAccount.address, value: SPONSOR_FUND_WEI, data: '0x' }],
    });
    const fundReceipt = await waitForReceipt(client, fundHash, RECEIPT_TIMEOUT_MS);
    assert.equal(fundReceipt.status, '0x1', 'funding tx should succeed');

    // Get balances before sponsored tx
    const executorBalanceBefore = await getBalance(client, executorAccount.address);
    const sponsorBalanceBefore = await getBalance(client, sponsorAccount.address);

    // Execute sponsored tx
    const intent = await evnodeSponsored.createIntent({ calls });
    const hash = await evnodeSponsored.sponsorAndSend({ intent });

    const receipt = await waitForReceipt(client, hash, RECEIPT_TIMEOUT_MS);
    assert.equal(receipt.status, '0x1', 'sponsored tx should succeed');

    // Verify sponsor paid gas, executor did not
    const executorBalanceAfter = await getBalance(client, executorAccount.address);
    const sponsorBalanceAfter = await getBalance(client, sponsorAccount.address);

    assert.equal(executorBalanceBefore, executorBalanceAfter, 'executor balance should not change');
    assert.ok(sponsorBalanceBefore > sponsorBalanceAfter, 'sponsor should have paid gas');
  }
});

// --- helpers ---

async function getBalance(client: any, address: `0x${string}`): Promise<bigint> {
  const hex = await client.request({
    method: 'eth_getBalance',
    params: [address, 'latest'],
  });
  return hexToBigInt(hex as Hex);
}

async function waitForReceipt(
  client: any,
  hash: Hex,
  timeoutMs: number,
): Promise<{ status: Hex; blockNumber: Hex }> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const receipt = await client.request({
      method: 'eth_getTransactionReceipt',
      params: [hash],
    });
    if (receipt) return receipt as { status: Hex; blockNumber: Hex };
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`Receipt not found for ${hash} within ${timeoutMs}ms`);
}
