import { createClient, hexToBigInt, http, type Hex, toHex, formatEther } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { randomBytes } from 'crypto';
import { createEvnodeClient, type Call } from '../evnode-viem.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const EXECUTOR_KEY = normalizeKey(
  process.env.EXECUTOR_PRIVATE_KEY ?? process.env.PRIVATE_KEY,
);
const TO_ADDRESS = process.env.TO_ADDRESS as `0x${string}` | undefined;
const SPONSOR_FUND_WEI = BigInt(process.env.SPONSOR_FUND_WEI ?? '10000000000000000'); // 0.01 ETH

if (!EXECUTOR_KEY) {
  throw new Error('Missing EXECUTOR_PRIVATE_KEY/PRIVATE_KEY');
}

const client = createClient({ transport: http(RPC_URL) });
const executorAccount = privateKeyToAccount(EXECUTOR_KEY);

const executor = {
  address: executorAccount.address,
  signHash: async (hash: Hex) => sign({ hash, privateKey: EXECUTOR_KEY }),
};

// For unsponsored txs, we use executor-only client
const evnodeUnsponsored = createEvnodeClient({
  client,
  executor,
});

async function main() {
  const to = TO_ADDRESS ?? executorAccount.address;

  console.log('executor', executorAccount.address);
  console.log('');

  // Run all flows
  await runUnsponsoredFlow('unsponsored-single', [call(to)]);
  await runUnsponsoredBatchFlow();
  await runSponsoredFlow('sponsored-single', [call(to)]);
  await runSponsoredFlow('sponsored-batch', [call(to), call(to)]);
}

async function runUnsponsoredFlow(name: string, calls: Call[]) {
  console.log(`\n== ${name} ==`);

  const executorBalanceBefore = await getBalance(executorAccount.address);
  console.log('executor balance before:', formatEther(executorBalanceBefore), 'ETH');

  const hash = await evnodeUnsponsored.send({ calls });
  console.log('submitted tx:', hash);

  const receipt = await pollReceipt(hash);
  if (receipt) {
    console.log('receipt status:', receipt.status, 'block:', receipt.blockNumber);

    const executorBalanceAfter = await getBalance(executorAccount.address);
    const executorSpent = executorBalanceBefore - executorBalanceAfter;
    console.log('executor balance after:', formatEther(executorBalanceAfter), 'ETH');
    console.log('executor spent (gas):', formatEther(executorSpent), 'ETH');
  } else {
    console.log('receipt not found yet');
  }
}

const TRANSFER_AMOUNT = BigInt('1000000000000000'); // 0.001 ETH

async function runUnsponsoredBatchFlow() {
  console.log('\n== unsponsored-batch ==');

  // Create 2 random recipient addresses
  const recipient1Key = toHex(randomBytes(32)) as `0x${string}`;
  const recipient2Key = toHex(randomBytes(32)) as `0x${string}`;
  const recipient1 = privateKeyToAccount(recipient1Key).address;
  const recipient2 = privateKeyToAccount(recipient2Key).address;

  console.log('recipient1:', recipient1);
  console.log('recipient2:', recipient2);

  // Get balances before
  const executorBalanceBefore = await getBalance(executorAccount.address);
  const recipient1Before = await getBalance(recipient1);
  const recipient2Before = await getBalance(recipient2);

  console.log('\n1. Balances before:');
  console.log('   executor:', formatEther(executorBalanceBefore), 'ETH');
  console.log('   recipient1:', formatEther(recipient1Before), 'ETH');
  console.log('   recipient2:', formatEther(recipient2Before), 'ETH');

  // Send batch: transfer 0.001 ETH to each recipient
  console.log('\n2. Sending batch (0.001 ETH to each recipient)...');
  const hash = await evnodeUnsponsored.send({
    calls: [
      { to: recipient1, value: TRANSFER_AMOUNT, data: '0x' },
      { to: recipient2, value: TRANSFER_AMOUNT, data: '0x' },
    ],
  });
  console.log('submitted tx:', hash);

  const receipt = await pollReceipt(hash);
  if (receipt) {
    console.log('receipt status:', receipt.status, 'block:', receipt.blockNumber);

    // Get balances after
    const executorBalanceAfter = await getBalance(executorAccount.address);
    const recipient1After = await getBalance(recipient1);
    const recipient2After = await getBalance(recipient2);

    console.log('\n3. Balances after:');
    console.log('   executor:', formatEther(executorBalanceAfter), 'ETH');
    console.log('   recipient1:', formatEther(recipient1After), 'ETH');
    console.log('   recipient2:', formatEther(recipient2After), 'ETH');

    // Verify transfers
    const executorSpent = executorBalanceBefore - executorBalanceAfter;
    const recipient1Received = recipient1After - recipient1Before;
    const recipient2Received = recipient2After - recipient2Before;
    const totalTransferred = TRANSFER_AMOUNT * 2n;
    const gasSpent = executorSpent - totalTransferred;

    console.log('\n4. Verification:');
    console.log('   executor total spent:', formatEther(executorSpent), 'ETH');
    console.log('   gas cost:', formatEther(gasSpent), 'ETH');
    console.log('   recipient1 received:', formatEther(recipient1Received), 'ETH');
    console.log('   recipient2 received:', formatEther(recipient2Received), 'ETH');

    if (recipient1Received === TRANSFER_AMOUNT && recipient2Received === TRANSFER_AMOUNT) {
      console.log('\n✓ VERIFIED: Both recipients received exactly 0.001 ETH');
    } else {
      console.log('\n✗ UNEXPECTED: Transfer amounts do not match');
    }
  } else {
    console.log('receipt not found yet');
  }
}

async function runSponsoredFlow(name: string, calls: Call[]) {
  console.log(`\n== ${name} ==`);

  // Create a fresh sponsor for each sponsored test
  const sponsorKey = toHex(randomBytes(32)) as `0x${string}`;
  const sponsorAccount = privateKeyToAccount(sponsorKey);

  console.log('sponsor address:', sponsorAccount.address);
  console.log('sponsor key:', sponsorKey);

  const sponsor = {
    address: sponsorAccount.address,
    signHash: async (hash: Hex) => sign({ hash, privateKey: sponsorKey }),
  };

  // Create evnode client with this sponsor
  const evnodeSponsored = createEvnodeClient({
    client,
    executor,
    sponsor,
  });

  // Step 1: Fund the sponsor
  console.log('\n1. Funding sponsor with', formatEther(SPONSOR_FUND_WEI), 'ETH...');
  const fundingHash = await evnodeUnsponsored.send({
    calls: [{ to: sponsorAccount.address, value: SPONSOR_FUND_WEI, data: '0x' }],
  });
  console.log('funding tx:', fundingHash);

  const fundingReceipt = await pollReceipt(fundingHash);
  if (!fundingReceipt) {
    console.log('ERROR: funding tx not mined');
    return;
  }
  console.log('funding tx mined in block:', fundingReceipt.blockNumber);

  // Step 2: Get balances before sponsored tx
  const executorBalanceBefore = await getBalance(executorAccount.address);
  const sponsorBalanceBefore = await getBalance(sponsorAccount.address);

  console.log('\n2. Balances before sponsored tx:');
  console.log('   executor:', formatEther(executorBalanceBefore), 'ETH');
  console.log('   sponsor:', formatEther(sponsorBalanceBefore), 'ETH');

  // Step 3: Execute sponsored tx
  console.log('\n3. Executing sponsored tx...');
  const intent = await evnodeSponsored.createIntent({ calls });
  const hash = await evnodeSponsored.sponsorAndSend({ intent });
  console.log('submitted tx:', hash);

  const receipt = await pollReceipt(hash);
  if (receipt) {
    console.log('receipt status:', receipt.status, 'block:', receipt.blockNumber);

    // Step 4: Get balances after and verify
    const executorBalanceAfter = await getBalance(executorAccount.address);
    const sponsorBalanceAfter = await getBalance(sponsorAccount.address);

    const executorDiff = executorBalanceBefore - executorBalanceAfter;
    const sponsorDiff = sponsorBalanceBefore - sponsorBalanceAfter;

    console.log('\n4. Balances after sponsored tx:');
    console.log('   executor:', formatEther(executorBalanceAfter), 'ETH');
    console.log('   sponsor:', formatEther(sponsorBalanceAfter), 'ETH');

    console.log('\n5. Balance changes:');
    console.log('   executor spent:', formatEther(executorDiff), 'ETH');
    console.log('   sponsor spent:', formatEther(sponsorDiff), 'ETH (should be gas cost)');

    // Verify sponsor paid gas
    if (sponsorDiff > 0n && executorDiff === 0n) {
      console.log('\n✓ VERIFIED: Sponsor paid gas, executor paid nothing');
    } else if (sponsorDiff > 0n) {
      console.log('\n✓ Sponsor paid gas:', formatEther(sponsorDiff), 'ETH');
      if (executorDiff > 0n) {
        console.log('  (executor also spent some, possibly from value transfer in calls)');
      }
    } else {
      console.log('\n✗ UNEXPECTED: Sponsor did not pay gas');
    }
  } else {
    console.log('receipt not found yet');
  }
}

function call(to: `0x${string}`): Call {
  return { to, value: 0n, data: '0x' };
}

async function getBalance(address: `0x${string}`): Promise<bigint> {
  const balanceHex = await client.request({
    method: 'eth_getBalance',
    params: [address, 'latest'],
  });
  return hexToBigInt(balanceHex as Hex);
}

async function pollReceipt(hash: Hex) {
  for (let i = 0; i < 20; i += 1) {
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
