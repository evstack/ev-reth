import { createClient, hexToBigInt, http, type Hex, toHex } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { randomBytes } from 'crypto';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const EXECUTOR_KEY = normalizeKey(
  process.env.EXECUTOR_PRIVATE_KEY ?? process.env.PRIVATE_KEY,
);
const SPONSOR_KEY = normalizeKey(process.env.SPONSOR_PRIVATE_KEY ?? '');
const TO_ADDRESS = process.env.TO_ADDRESS as `0x${string}` | undefined;
const AUTO_SPONSOR =
  process.env.AUTO_SPONSOR === '1' || process.env.AUTO_SPONSOR === 'true';
const FUND_SPONSOR =
  process.env.FUND_SPONSOR === '1' || process.env.FUND_SPONSOR === 'true';
const SPONSOR_MIN_BALANCE_WEI = BigInt(process.env.SPONSOR_MIN_BALANCE_WEI ?? '0');
const SPONSOR_FUND_WEI = BigInt(process.env.SPONSOR_FUND_WEI ?? '10000000000000000');

if (!EXECUTOR_KEY) {
  throw new Error('Missing EXECUTOR_PRIVATE_KEY/PRIVATE_KEY');
}

const autoSponsorKey = AUTO_SPONSOR ? toHex(randomBytes(32)) : undefined;
const sponsorKey = (SPONSOR_KEY || autoSponsorKey || EXECUTOR_KEY) as `0x${string}`;
const client = createClient({ transport: http(RPC_URL) });

const executorAccount = privateKeyToAccount(EXECUTOR_KEY);
const sponsorAccount = privateKeyToAccount(sponsorKey);

const evnode = createEvnodeClient({
  client,
  executor: {
    address: executorAccount.address,
    signHash: async (hash: Hex) => sign({ hash, privateKey: EXECUTOR_KEY }),
  },
  sponsor: {
    address: sponsorAccount.address,
    signHash: async (hash: Hex) => sign({ hash, privateKey: sponsorKey }),
  },
});

async function main() {
  const to = TO_ADDRESS ?? executorAccount.address;
  console.log('executor', executorAccount.address);
  console.log('sponsor', sponsorAccount.address);
  if (autoSponsorKey) {
    console.log('auto sponsor key', sponsorKey);
  }

  await maybeFundSponsor();
  const intent = await evnode.createIntent({
    calls: [{ to, value: 0n, data: '0x' }],
  });
  const hash = await evnode.sponsorAndSend({ intent });
  console.log('submitted sponsored tx:', hash);
  const receipt = await pollReceipt(hash);
  if (receipt) {
    console.log('receipt status:', receipt.status, 'block:', receipt.blockNumber);
  } else {
    console.log('receipt not found yet');
  }
}

async function maybeFundSponsor() {
  if (sponsorAccount.address === executorAccount.address) return;
  if (!FUND_SPONSOR && SPONSOR_MIN_BALANCE_WEI === 0n) return;
  const balanceHex = await client.request({
    method: 'eth_getBalance',
    params: [sponsorAccount.address, 'latest'],
  });
  const balance = hexToBigInt(balanceHex as Hex);
  if (!FUND_SPONSOR && balance >= SPONSOR_MIN_BALANCE_WEI) return;
  const target = SPONSOR_MIN_BALANCE_WEI > 0n ? SPONSOR_MIN_BALANCE_WEI : SPONSOR_FUND_WEI;
  const amount = target > balance ? target - balance : SPONSOR_FUND_WEI;
  if (amount <= 0n) return;
  console.log('funding sponsor with', amount.toString(), 'wei');
  const hash = await evnode.send({
    calls: [{ to: sponsorAccount.address, value: amount, data: '0x' }],
  });
  const receipt = await pollReceiptWithTimeout(hash, 30);
  if (!receipt) throw new Error('sponsor funding tx not mined');
}

async function pollReceipt(hash: Hex) {
  for (let i = 0; i < 15; i += 1) {
    const receipt = await client.request({
      method: 'eth_getTransactionReceipt',
      params: [hash],
    });
    if (receipt) return receipt as { status: Hex; blockNumber: Hex };
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return null;
}

async function pollReceiptWithTimeout(hash: Hex, attempts: number) {
  for (let i = 0; i < attempts; i += 1) {
    const receipt = await client.request({
      method: 'eth_getTransactionReceipt',
      params: [hash],
    });
    if (receipt) return receipt as { status: Hex; blockNumber: Hex };
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return null;
}

function normalizeKey(key?: string): `0x${string}` | '' | undefined {
  if (!key) return undefined;
  return key.startsWith('0x') ? (key as `0x${string}`) : (`0x${key}` as `0x${string}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
