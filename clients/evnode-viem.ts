import {
  type AccessList,
  type Address,
  type Client,
  type Hex,
  type Signature,
  bytesToHex,
  concat,
  defineTransaction,
  fromRlp,
  hexToBigInt,
  hexToBytes,
  hexToSignature,
  isHex,
  keccak256,
  recoverAddress,
  toHex,
  toRlp,
} from 'viem';

export const EVNODE_TX_TYPE = 0x76;
export const EVNODE_EXECUTOR_DOMAIN = 0x76;
export const EVNODE_SPONSOR_DOMAIN = 0x78;

const EMPTY_BYTES = '0x' as const;
const TX_TYPE_HEX = toHex(EVNODE_TX_TYPE, { size: 1 });
const EXECUTOR_DOMAIN_HEX = toHex(EVNODE_EXECUTOR_DOMAIN, { size: 1 });
const SPONSOR_DOMAIN_HEX = toHex(EVNODE_SPONSOR_DOMAIN, { size: 1 });

type RlpValue = Hex | RlpValue[];

export interface Call {
  to: Address | null;
  value: bigint;
  data: Hex;
}

export interface EvNodeTransaction {
  chainId: bigint;
  nonce: bigint;
  maxPriorityFeePerGas: bigint;
  maxFeePerGas: bigint;
  gasLimit: bigint;
  calls: Call[];
  accessList: AccessList;
  feePayerSignature?: Signature;
}

export interface EvNodeSignedTransaction {
  transaction: EvNodeTransaction;
  executorSignature: Signature;
}

export interface SponsorableIntent {
  tx: EvNodeTransaction;
  executorSignature: Signature;
  executorAddress: Address;
}

export interface HashSigner {
  address: Address;
  // Must sign the raw 32-byte hash without EIP-191 prefixing.
  signHash: (hash: Hex) => Promise<Signature>;
}

export interface EvnodeClientOptions {
  client: Client;
  executor?: HashSigner;
  sponsor?: HashSigner;
}

export interface EvnodeSendArgs {
  calls: Call[];
  executor?: HashSigner;
  chainId?: bigint;
  nonce?: bigint;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
  gasLimit?: bigint;
  accessList?: AccessList;
}

export interface EvnodeIntentArgs {
  calls: Call[];
  executor?: HashSigner;
  chainId?: bigint;
  nonce?: bigint;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
  gasLimit?: bigint;
  accessList?: AccessList;
}

export interface EvnodeSponsorArgs {
  intent: SponsorableIntent;
  sponsor?: HashSigner;
}

export function encodeSignedTransaction(signedTx: EvNodeSignedTransaction): Hex {
  const fields = buildPayloadFields(signedTx.transaction, true);
  const execSig = normalizeSignature(signedTx.executorSignature);
  const envelope = toRlp([
    ...fields,
    execSig.v,
    hexToBigInt(execSig.r),
    hexToBigInt(execSig.s),
  ]);
  return concat([TX_TYPE_HEX, envelope]);
}

export function decodeEvNodeTransaction(encoded: Hex): EvNodeSignedTransaction {
  const bytes = hexToBytes(encoded);
  if (bytes.length === 0 || bytes[0] !== EVNODE_TX_TYPE) {
    throw new Error('Invalid EvNode transaction type');
  }

  const decoded = fromRlp(bytesToHex(bytes.slice(1))) as RlpValue;
  if (!Array.isArray(decoded)) {
    throw new Error('Invalid EvNode transaction payload');
  }

  if (decoded.length !== 11) {
    throw new Error('Invalid EvNode transaction length');
  }

  const [
    chainId,
    nonce,
    maxPriorityFeePerGas,
    maxFeePerGas,
    gasLimit,
    calls,
    accessList,
    feePayerSignature,
    v,
    r,
    s,
  ] = decoded;

  const transaction: EvNodeTransaction = {
    chainId: hexToBigIntSafe(chainId),
    nonce: hexToBigIntSafe(nonce),
    maxPriorityFeePerGas: hexToBigIntSafe(maxPriorityFeePerGas),
    maxFeePerGas: hexToBigIntSafe(maxFeePerGas),
    gasLimit: hexToBigIntSafe(gasLimit),
    calls: decodeCalls(calls),
    accessList: decodeAccessList(accessList),
    feePayerSignature: decodeSignature(feePayerSignature),
  };

  const executorSignature = signatureFromParts(v, r, s);
  return { transaction, executorSignature };
}

export function computeExecutorSigningHash(tx: EvNodeTransaction): Hex {
  const payload = toRlp(buildPayloadFields(tx, false));
  return keccak256(concat([EXECUTOR_DOMAIN_HEX, payload]));
}

export function computeSponsorSigningHash(
  tx: EvNodeTransaction,
  executorAddress: Address,
): Hex {
  const payload = toRlp(buildPayloadFields(tx, false));
  return keccak256(concat([SPONSOR_DOMAIN_HEX, executorAddress, payload]));
}

export function computeTxHash(signedTx: EvNodeSignedTransaction): Hex {
  return keccak256(encodeSignedTransaction(signedTx));
}

export async function recoverExecutor(
  signedTx: EvNodeSignedTransaction,
): Promise<Address> {
  const hash = computeExecutorSigningHash(signedTx.transaction);
  return recoverAddress({ hash, signature: normalizeSignature(signedTx.executorSignature) });
}

export async function recoverSponsor(
  tx: EvNodeTransaction,
  executorAddress: Address,
): Promise<Address | null> {
  if (!tx.feePayerSignature) return null;
  const hash = computeSponsorSigningHash(tx, executorAddress);
  return recoverAddress({ hash, signature: normalizeSignature(tx.feePayerSignature) });
}

export async function signAsExecutor(
  tx: EvNodeTransaction,
  signer: HashSigner,
): Promise<Signature> {
  const hash = computeExecutorSigningHash(tx);
  return signer.signHash(hash);
}

export async function signAsSponsor(
  tx: EvNodeTransaction,
  executorAddress: Address,
  signer: HashSigner,
): Promise<Signature> {
  const hash = computeSponsorSigningHash(tx, executorAddress);
  return signer.signHash(hash);
}

export function estimateIntrinsicGas(calls: Call[]): bigint {
  let gas = 21000n;

  for (const call of calls) {
    if (call.to === null) gas += 32000n;

    for (const byte of hexToBytes(call.data)) {
      gas += byte === 0 ? 4n : 16n;
    }
  }

  return gas;
}

export function validateEvNodeTx(tx: EvNodeTransaction): void {
  if (tx.calls.length === 0) {
    throw new Error('EvNode transaction must include at least one call');
  }

  for (let i = 1; i < tx.calls.length; i += 1) {
    if (tx.calls[i].to === null) {
      throw new Error('Only the first call may be CREATE');
    }
  }
}

export function evnodeActions(client: Client) {
  return {
    async sendEvNodeTransaction(args: {
      calls: Call[];
      executor: HashSigner;
      chainId?: bigint;
      nonce?: bigint;
      maxFeePerGas?: bigint;
      maxPriorityFeePerGas?: bigint;
      gasLimit?: bigint;
      accessList?: AccessList;
    }): Promise<Hex> {
      const base = await resolveBaseFields(client, args.executor.address, {
        chainId: args.chainId,
        nonce: args.nonce,
        maxFeePerGas: args.maxFeePerGas,
        maxPriorityFeePerGas: args.maxPriorityFeePerGas,
        gasLimit: args.gasLimit,
        accessList: args.accessList,
      }, args.calls);

      const tx: EvNodeTransaction = {
        ...base,
        calls: args.calls,
        feePayerSignature: undefined,
      };

      validateEvNodeTx(tx);

      const executorSignature = await signAsExecutor(tx, args.executor);
      const signedTx: EvNodeSignedTransaction = {
        transaction: tx,
        executorSignature,
      };

      const serialized = encodeSignedTransaction(signedTx);
      return client.request({
        method: 'eth_sendRawTransaction',
        params: [serialized],
      }) as Promise<Hex>;
    },

    async createSponsorableIntent(args: {
      calls: Call[];
      executor: HashSigner;
      chainId?: bigint;
      nonce?: bigint;
      maxFeePerGas?: bigint;
      maxPriorityFeePerGas?: bigint;
      gasLimit?: bigint;
      accessList?: AccessList;
    }): Promise<SponsorableIntent> {
      const base = await resolveBaseFields(client, args.executor.address, {
        chainId: args.chainId,
        nonce: args.nonce,
        maxFeePerGas: args.maxFeePerGas,
        maxPriorityFeePerGas: args.maxPriorityFeePerGas,
        gasLimit: args.gasLimit,
        accessList: args.accessList,
      }, args.calls);

      const tx: EvNodeTransaction = {
        ...base,
        calls: args.calls,
        feePayerSignature: undefined,
      };

      validateEvNodeTx(tx);

      const executorSignature = await signAsExecutor(tx, args.executor);

      return {
        tx,
        executorSignature,
        executorAddress: args.executor.address,
      };
    },

    async sponsorIntent(args: {
      intent: SponsorableIntent;
      sponsor: HashSigner;
    }): Promise<EvNodeSignedTransaction> {
      const sponsorSignature = await signAsSponsor(
        args.intent.tx,
        args.intent.executorAddress,
        args.sponsor,
      );

      return {
        transaction: {
          ...args.intent.tx,
          feePayerSignature: sponsorSignature,
        },
        executorSignature: args.intent.executorSignature,
      };
    },

    serializeEvNodeTransaction(signedTx: EvNodeSignedTransaction): Hex {
      return encodeSignedTransaction(signedTx);
    },

    deserializeEvNodeTransaction(encoded: Hex): EvNodeSignedTransaction {
      return decodeEvNodeTransaction(encoded);
    },
  };
}

export function createEvnodeClient(options: EvnodeClientOptions) {
  const actions = evnodeActions(options.client);
  let defaultExecutor = options.executor;
  let defaultSponsor = options.sponsor;

  const requireExecutor = (executor?: HashSigner) => {
    const resolved = executor ?? defaultExecutor;
    if (!resolved) throw new Error('Executor signer is required');
    return resolved;
  };

  const requireSponsor = (sponsor?: HashSigner) => {
    const resolved = sponsor ?? defaultSponsor;
    if (!resolved) throw new Error('Sponsor signer is required');
    return resolved;
  };

  return {
    client: options.client,
    actions,
    setDefaultExecutor(executor: HashSigner) {
      defaultExecutor = executor;
    },
    setDefaultSponsor(sponsor: HashSigner) {
      defaultSponsor = sponsor;
    },
    send(args: EvnodeSendArgs): Promise<Hex> {
      return actions.sendEvNodeTransaction({
        ...args,
        executor: requireExecutor(args.executor),
      });
    },
    createIntent(args: EvnodeIntentArgs): Promise<SponsorableIntent> {
      return actions.createSponsorableIntent({
        ...args,
        executor: requireExecutor(args.executor),
      });
    },
    sponsorIntent(args: EvnodeSponsorArgs): Promise<EvNodeSignedTransaction> {
      return actions.sponsorIntent({
        intent: args.intent,
        sponsor: requireSponsor(args.sponsor),
      });
    },
    async sponsorAndSend(args: EvnodeSponsorArgs): Promise<Hex> {
      const signed = await actions.sponsorIntent({
        intent: args.intent,
        sponsor: requireSponsor(args.sponsor),
      });
      const serialized = actions.serializeEvNodeTransaction(signed);
      return options.client.request({
        method: 'eth_sendRawTransaction',
        params: [serialized],
      }) as Promise<Hex>;
    },
    serialize: actions.serializeEvNodeTransaction,
    deserialize: actions.deserializeEvNodeTransaction,
  };
}

export const evnodeSerializer = defineTransaction({
  type: 'evnode',
  typeId: EVNODE_TX_TYPE,
  serialize: (tx) => encodeSignedTransaction(tx as EvNodeSignedTransaction),
  deserialize: (bytes) => decodeEvNodeTransaction(bytes as Hex),
});

export function hashSignerFromRpcClient(
  client: Client,
  address: Address,
): HashSigner {
  return {
    address,
    signHash: async (hash) => {
      // eth_sign is expected to sign raw bytes (no EIP-191 prefix).
      const signature = await client.request({
        method: 'eth_sign',
        params: [address, hash],
      });
      if (!isHex(signature)) {
        throw new Error('eth_sign returned non-hex signature');
      }
      return signature;
    },
  };
}

function buildPayloadFields(tx: EvNodeTransaction, includeSponsorSig: boolean): RlpValue[] {
  return [
    tx.chainId,
    tx.nonce,
    tx.maxPriorityFeePerGas,
    tx.maxFeePerGas,
    tx.gasLimit,
    encodeCalls(tx.calls),
    encodeAccessList(tx.accessList),
    includeSponsorSig && tx.feePayerSignature
      ? encodeSignatureList(tx.feePayerSignature)
      : EMPTY_BYTES,
  ];
}

function encodeCalls(calls: Call[]): RlpValue[] {
  return calls.map((call) => [
    call.to ?? EMPTY_BYTES,
    call.value,
    call.data,
  ]);
}

function decodeCalls(value: RlpValue): Call[] {
  if (!Array.isArray(value)) {
    throw new Error('Invalid EvNode calls encoding');
  }

  return value.map((call) => {
    if (!Array.isArray(call) || call.length !== 3) {
      throw new Error('Invalid EvNode call encoding');
    }

    const [to, val, data] = call;
    if (!isHex(to) || !isHex(val) || !isHex(data)) {
      throw new Error('Invalid EvNode call values');
    }

    return {
      to: to === EMPTY_BYTES ? null : (to as Address),
      value: hexToBigIntSafe(val),
      data,
    };
  });
}

function encodeAccessList(accessList: AccessList): RlpValue[] {
  return accessList.map((item) => [item.address, item.storageKeys]);
}

function decodeAccessList(value: RlpValue): AccessList {
  if (!Array.isArray(value)) {
    throw new Error('Invalid access list encoding');
  }

  return value.map((item) => {
    if (!Array.isArray(item) || item.length !== 2) {
      throw new Error('Invalid access list item encoding');
    }

    const [address, storageKeys] = item;
    if (!isHex(address) || !Array.isArray(storageKeys)) {
      throw new Error('Invalid access list values');
    }

    return {
      address: address as Address,
      storageKeys: storageKeys.map((key) => {
        if (!isHex(key)) throw new Error('Invalid storage key');
        return key;
      }),
    };
  });
}

function encodeSignatureList(signature: Signature): RlpValue[] {
  const normalized = normalizeSignature(signature);
  return [
    normalized.v,
    hexToBigInt(normalized.r),
    hexToBigInt(normalized.s),
  ];
}

function decodeSignature(value: RlpValue): Signature | undefined {
  if (value === EMPTY_BYTES) return undefined;

  if (!Array.isArray(value) || value.length !== 3) {
    throw new Error('Invalid sponsor signature encoding');
  }

  const [v, r, s] = value;
  return signatureFromParts(v, r, s);
}

function signatureFromParts(v: RlpValue, r: RlpValue, s: RlpValue): Signature {
  if (!isHex(v) || !isHex(r) || !isHex(s)) {
    throw new Error('Invalid signature fields');
  }

  const vNumber = Number(hexToBigIntSafe(v));
  if (vNumber !== 0 && vNumber !== 1) {
    throw new Error('Invalid signature v value');
  }

  return {
    v: vNumber,
    r: padTo32Bytes(r),
    s: padTo32Bytes(s),
  };
}

function normalizeSignature(signature: Signature): { v: number; r: Hex; s: Hex } {
  const parsed = typeof signature === 'string' ? hexToSignature(signature) : signature;

  const v = Number(parsed.v);
  const normalizedV = v === 27 || v === 28 ? v - 27 : v;
  if (normalizedV !== 0 && normalizedV !== 1) {
    throw new Error('Invalid signature v value');
  }

  return {
    v: normalizedV,
    r: padTo32Bytes(parsed.r),
    s: padTo32Bytes(parsed.s),
  };
}

function padTo32Bytes(value: Hex): Hex {
  return toHex(hexToBigIntSafe(value), { size: 32 });
}

function hexToBigIntSafe(value: RlpValue): bigint {
  if (!isHex(value)) throw new Error('Invalid hex value');
  return value === EMPTY_BYTES ? 0n : hexToBigInt(value);
}

async function resolveBaseFields(
  client: Client,
  address: Address,
  overrides: {
    chainId?: bigint;
    nonce?: bigint;
    maxFeePerGas?: bigint;
    maxPriorityFeePerGas?: bigint;
    gasLimit?: bigint;
    accessList?: AccessList;
  },
  calls: Call[],
): Promise<Omit<EvNodeTransaction, 'calls' | 'feePayerSignature'>> {
  const chainId = overrides.chainId ?? (await fetchChainId(client));
  const nonce = overrides.nonce ?? (await fetchNonce(client, address));
  const maxPriorityFeePerGas =
    overrides.maxPriorityFeePerGas ?? (await fetchMaxPriorityFee(client));
  const maxFeePerGas = overrides.maxFeePerGas ?? (await fetchGasPrice(client));
  const gasLimit = overrides.gasLimit ?? estimateIntrinsicGas(calls);
  const accessList = overrides.accessList ?? [];

  return {
    chainId,
    nonce,
    maxPriorityFeePerGas,
    maxFeePerGas,
    gasLimit,
    accessList,
  };
}

async function fetchChainId(client: Client): Promise<bigint> {
  const result = await client.request({ method: 'eth_chainId' });
  if (!isHex(result)) throw new Error('eth_chainId returned non-hex');
  return hexToBigIntSafe(result);
}

async function fetchNonce(client: Client, address: Address): Promise<bigint> {
  const result = await client.request({
    method: 'eth_getTransactionCount',
    params: [address, 'pending'],
  });
  if (!isHex(result)) throw new Error('eth_getTransactionCount returned non-hex');
  return hexToBigIntSafe(result);
}

async function fetchMaxPriorityFee(client: Client): Promise<bigint> {
  try {
    const result = await client.request({ method: 'eth_maxPriorityFeePerGas' });
    if (!isHex(result)) throw new Error('eth_maxPriorityFeePerGas returned non-hex');
    return hexToBigIntSafe(result);
  } catch {
    return 0n;
  }
}

async function fetchGasPrice(client: Client): Promise<bigint> {
  const result = await client.request({ method: 'eth_gasPrice' });
  if (!isHex(result)) throw new Error('eth_gasPrice returned non-hex');
  return hexToBigIntSafe(result);
}
