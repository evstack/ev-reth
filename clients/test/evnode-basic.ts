import { createClient, http } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../evnode-viem.ts';

const RPC_URL = 'http://localhost:8545';
const PRIVATE_KEY =
  (process.env.PRIVATE_KEY?.startsWith('0x')
    ? process.env.PRIVATE_KEY
    : process.env.PRIVATE_KEY
      ? `0x${process.env.PRIVATE_KEY}`
      : undefined) ??
  '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const TO_ADDRESS = process.env.TO_ADDRESS as `0x${string}` | undefined;

const client = createClient({
  transport: http(RPC_URL),
});

const account = privateKeyToAccount(PRIVATE_KEY);

const executor = {
  address: account.address,
  signHash: async (hash: `0x${string}`) => sign({ hash, privateKey: PRIVATE_KEY }),
};

const evnode = createEvnodeClient({
  client,
  executor,
});

async function main() {
  const to = TO_ADDRESS ?? account.address;
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
