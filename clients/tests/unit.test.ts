import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { privateKeyToAccount, sign } from 'viem/accounts';
import type { Hex, Address } from 'viem';

import {
  encodeSignedTransaction,
  decodeEvNodeTransaction,
  computeExecutorSigningHash,
  computeSponsorSigningHash,
  computeTxHash,
  estimateIntrinsicGas,
  validateEvNodeTx,
  normalizeSignature,
  signAsExecutor,
  signAsSponsor,
  recoverExecutor,
  recoverSponsor,
  type EvNodeTransaction,
  type EvNodeSignedTransaction,
  type Call,
  type HashSigner,
} from '../src/index.ts';

const TEST_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80' as const;
const TEST_ACCOUNT = privateKeyToAccount(TEST_KEY);

function makeHashSigner(key: typeof TEST_KEY): HashSigner {
  const account = privateKeyToAccount(key);
  return {
    address: account.address,
    signHash: async (hash: Hex) => sign({ hash, privateKey: key }),
  };
}

function makeTx(overrides: Partial<EvNodeTransaction> = {}): EvNodeTransaction {
  return {
    chainId: 1n,
    nonce: 0n,
    maxPriorityFeePerGas: 1000000000n,
    maxFeePerGas: 2000000000n,
    gasLimit: 21000n,
    calls: [{ to: TEST_ACCOUNT.address, value: 0n, data: '0x' }],
    accessList: [],
    ...overrides,
  };
}

// --- estimateIntrinsicGas ---

describe('estimateIntrinsicGas', () => {
  it('returns base + per-call gas for a simple call', () => {
    const calls: Call[] = [{ to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' }];
    // base (21000) + 1 call (21000) = 42000
    assert.equal(estimateIntrinsicGas(calls), 42000n);
  });

  it('adds CREATE gas when to is null', () => {
    const calls: Call[] = [{ to: null, value: 0n, data: '0x' }];
    // base (21000) + 1 call (21000) + CREATE (32000) = 74000
    assert.equal(estimateIntrinsicGas(calls), 74000n);
  });

  it('charges 4 gas per zero byte, 16 per non-zero byte', () => {
    const calls: Call[] = [{ to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x00ff00' }];
    // base (21000) + 1 call (21000) + 2 zero bytes (8) + 1 non-zero byte (16) = 42024
    assert.equal(estimateIntrinsicGas(calls), 42024n);
  });

  it('handles multiple calls', () => {
    const calls: Call[] = [
      { to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' },
      { to: '0x0000000000000000000000000000000000000002', value: 0n, data: '0x' },
    ];
    // base (21000) + 2 calls (42000) = 63000
    assert.equal(estimateIntrinsicGas(calls), 63000n);
  });
});

// --- validateEvNodeTx ---

describe('validateEvNodeTx', () => {
  it('throws on empty calls', () => {
    const tx = makeTx({ calls: [] });
    assert.throws(() => validateEvNodeTx(tx), /at least one call/);
  });

  it('allows CREATE as first call', () => {
    const tx = makeTx({ calls: [{ to: null, value: 0n, data: '0x6000' }] });
    assert.doesNotThrow(() => validateEvNodeTx(tx));
  });

  it('rejects CREATE in non-first position', () => {
    const tx = makeTx({
      calls: [
        { to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' },
        { to: null, value: 0n, data: '0x6000' },
      ],
    });
    assert.throws(() => validateEvNodeTx(tx), /Only the first call may be CREATE/);
  });

  it('accepts multiple regular calls', () => {
    const tx = makeTx({
      calls: [
        { to: '0x0000000000000000000000000000000000000001', value: 0n, data: '0x' },
        { to: '0x0000000000000000000000000000000000000002', value: 0n, data: '0x' },
      ],
    });
    assert.doesNotThrow(() => validateEvNodeTx(tx));
  });
});

// --- normalizeSignature ---

describe('normalizeSignature', () => {
  it('normalizes v=27 to yParity=0', () => {
    const sig = {
      v: 27n,
      r: '0x0000000000000000000000000000000000000000000000000000000000000001' as Hex,
      s: '0x0000000000000000000000000000000000000000000000000000000000000002' as Hex,
    };
    const normalized = normalizeSignature(sig);
    assert.equal(normalized.yParity, 0);
  });

  it('normalizes v=28 to yParity=1', () => {
    const sig = {
      v: 28n,
      r: '0x0000000000000000000000000000000000000000000000000000000000000001' as Hex,
      s: '0x0000000000000000000000000000000000000000000000000000000000000002' as Hex,
    };
    const normalized = normalizeSignature(sig);
    assert.equal(normalized.yParity, 1);
  });

  it('keeps v=0 as yParity=0', () => {
    const sig = {
      v: 0n,
      r: '0x0000000000000000000000000000000000000000000000000000000000000001' as Hex,
      s: '0x0000000000000000000000000000000000000000000000000000000000000002' as Hex,
    };
    const normalized = normalizeSignature(sig);
    assert.equal(normalized.yParity, 0);
  });

  it('pads r and s to 32 bytes', () => {
    const sig = {
      v: 0n,
      r: '0x01' as Hex,
      s: '0x02' as Hex,
    };
    const normalized = normalizeSignature(sig);
    assert.equal(normalized.r.length, 66); // 0x + 64 hex chars
    assert.equal(normalized.s.length, 66);
  });
});

// --- encode/decode roundtrip ---

describe('encode/decode roundtrip', () => {
  it('roundtrips a simple signed transaction', async () => {
    const tx = makeTx();
    const signer = makeHashSigner(TEST_KEY);
    const executorSignature = await signAsExecutor(tx, signer);

    const signedTx: EvNodeSignedTransaction = { transaction: tx, executorSignature };
    const encoded = encodeSignedTransaction(signedTx);
    const decoded = decodeEvNodeTransaction(encoded);

    assert.equal(decoded.transaction.chainId, tx.chainId);
    assert.equal(decoded.transaction.nonce, tx.nonce);
    assert.equal(decoded.transaction.maxPriorityFeePerGas, tx.maxPriorityFeePerGas);
    assert.equal(decoded.transaction.maxFeePerGas, tx.maxFeePerGas);
    assert.equal(decoded.transaction.gasLimit, tx.gasLimit);
    assert.equal(decoded.transaction.calls.length, 1);
    assert.equal(decoded.transaction.calls[0].to?.toLowerCase(), tx.calls[0].to?.toLowerCase());
    assert.equal(decoded.transaction.calls[0].value, tx.calls[0].value);
    assert.equal(decoded.transaction.calls[0].data, tx.calls[0].data);
    assert.equal(decoded.transaction.accessList.length, 0);
    assert.equal(decoded.transaction.feePayerSignature, undefined);
  });

  it('roundtrips a transaction with access list', async () => {
    const tx = makeTx({
      accessList: [{
        address: '0x0000000000000000000000000000000000000001',
        storageKeys: ['0x0000000000000000000000000000000000000000000000000000000000000001'],
      }],
    });
    const signer = makeHashSigner(TEST_KEY);
    const executorSignature = await signAsExecutor(tx, signer);

    const signedTx: EvNodeSignedTransaction = { transaction: tx, executorSignature };
    const encoded = encodeSignedTransaction(signedTx);
    const decoded = decodeEvNodeTransaction(encoded);

    assert.equal(decoded.transaction.accessList.length, 1);
    assert.equal(decoded.transaction.accessList[0].address, '0x0000000000000000000000000000000000000001');
    assert.equal(decoded.transaction.accessList[0].storageKeys.length, 1);
  });

  it('roundtrips a CREATE transaction', async () => {
    const tx = makeTx({
      calls: [{ to: null, value: 0n, data: '0x6000600060006000' }],
    });
    const signer = makeHashSigner(TEST_KEY);
    const executorSignature = await signAsExecutor(tx, signer);

    const signedTx: EvNodeSignedTransaction = { transaction: tx, executorSignature };
    const encoded = encodeSignedTransaction(signedTx);
    const decoded = decodeEvNodeTransaction(encoded);

    assert.equal(decoded.transaction.calls[0].to, null);
    assert.equal(decoded.transaction.calls[0].data, '0x6000600060006000');
  });

  it('produces a deterministic tx hash', async () => {
    const tx = makeTx();
    const signer = makeHashSigner(TEST_KEY);
    const executorSignature = await signAsExecutor(tx, signer);
    const signedTx: EvNodeSignedTransaction = { transaction: tx, executorSignature };

    const hash1 = computeTxHash(signedTx);
    const hash2 = computeTxHash(signedTx);
    assert.equal(hash1, hash2);
    assert.ok(hash1.startsWith('0x'));
    assert.equal(hash1.length, 66);
  });
});

// --- signing hashes ---

describe('signing hashes', () => {
  it('executor signing hash is deterministic', () => {
    const tx = makeTx();
    assert.equal(computeExecutorSigningHash(tx), computeExecutorSigningHash(tx));
  });

  it('different txs produce different executor hashes', () => {
    const tx1 = makeTx({ nonce: 0n });
    const tx2 = makeTx({ nonce: 1n });
    assert.notEqual(computeExecutorSigningHash(tx1), computeExecutorSigningHash(tx2));
  });

  it('sponsor signing hash is deterministic', () => {
    const tx = makeTx();
    const addr = TEST_ACCOUNT.address;
    assert.equal(computeSponsorSigningHash(tx, addr), computeSponsorSigningHash(tx, addr));
  });

  it('executor and sponsor hashes differ for same tx', () => {
    const tx = makeTx();
    const addr = TEST_ACCOUNT.address;
    assert.notEqual(computeExecutorSigningHash(tx), computeSponsorSigningHash(tx, addr));
  });
});

// --- sign and recover ---

describe('sign and recover', () => {
  it('recovers executor address from signed tx', async () => {
    const tx = makeTx();
    const signer = makeHashSigner(TEST_KEY);
    const executorSignature = await signAsExecutor(tx, signer);
    const signedTx: EvNodeSignedTransaction = { transaction: tx, executorSignature };

    const recovered = await recoverExecutor(signedTx);
    assert.equal(recovered.toLowerCase(), TEST_ACCOUNT.address.toLowerCase());
  });

  it('recovers sponsor address from sponsored tx', async () => {
    const SPONSOR_KEY = '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d' as const;
    const sponsorAccount = privateKeyToAccount(SPONSOR_KEY);

    const tx = makeTx();
    const executorSigner = makeHashSigner(TEST_KEY);
    const sponsorSigner = makeHashSigner(SPONSOR_KEY);

    const sponsorSig = await signAsSponsor(tx, executorSigner.address, sponsorSigner);
    const sponsoredTx: EvNodeTransaction = { ...tx, feePayerSignature: sponsorSig };

    const recovered = await recoverSponsor(sponsoredTx, executorSigner.address);
    assert.ok(recovered);
    assert.equal(recovered!.toLowerCase(), sponsorAccount.address.toLowerCase());
  });

  it('recoverSponsor returns null when no sponsor signature', async () => {
    const tx = makeTx();
    const recovered = await recoverSponsor(tx, TEST_ACCOUNT.address);
    assert.equal(recovered, null);
  });
});

// --- decode errors ---

describe('decode errors', () => {
  it('rejects wrong tx type', () => {
    assert.throws(() => decodeEvNodeTransaction('0xff'), /Invalid EvNode transaction type/);
  });

  it('rejects empty input', () => {
    assert.throws(() => decodeEvNodeTransaction('0x'), /Invalid EvNode transaction type/);
  });
});
