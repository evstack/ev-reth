import { createClient, http } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../evnode-viem.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const EXECUTOR_KEY = normalizeKey(
  process.env.EXECUTOR_PRIVATE_KEY ?? process.env.PRIVATE_KEY,
);
const TO_ADDRESS = process.env.TO_ADDRESS as `0x${string}` | undefined;

if (!EXECUTOR_KEY) {
  throw new Error('Missing EXECUTOR_PRIVATE_KEY or PRIVATE_KEY');
}

function normalizeKey(key?: string): `0x${string}` | undefined {
  if (!key) return undefined;
  return key.startsWith('0x') ? (key as `0x${string}`) : (`0x${key}` as `0x${string}`);
}

const client = createClient({
  transport: http(RPC_URL),
});

const executorAccount = privateKeyToAccount(EXECUTOR_KEY);

const executor = {
  address: executorAccount.address,
  signHash: async (hash: `0x${string}`) => sign({ hash, privateKey: EXECUTOR_KEY }),
};

const evnode = createEvnodeClient({
  client,
  executor,
});

async function main() {
  const to = TO_ADDRESS ?? executorAccount.address;
  const hash = await evnode.send({
    calls: [
      {
        to,
        value: 0n,
        data: '0x',
      },
    ],
  });

  console.log('submitted tx:', hash);

  const receipt = await pollReceipt(hash);
  if (receipt) {
    console.log('receipt status:', receipt.status, 'block:', receipt.blockNumber);
  } else {
    console.log('receipt not found yet');
  }
}

async function pollReceipt(hash: `0x${string}`) {
  for (let i = 0; i < 12; i += 1) {
    const receipt = await client.request({
      method: 'eth_getTransactionReceipt',
      params: [hash],
    });
    if (receipt) return receipt as { status: `0x${string}`; blockNumber: `0x${string}` };
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  return null;
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
