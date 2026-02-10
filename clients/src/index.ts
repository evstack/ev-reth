import {
  type AccessList,
  type Address,
  type Client,
  type Hex,
  type Signature,
  bytesToHex,
  concat,
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

const BASE_TX_GAS = 21000n;
// Extra gas charged when a call deploys a new contract (to === null)
const CREATE_GAS = 32000n;
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

type EvnodeSendArgsWithExecutor = Omit<EvnodeSendArgs, 'executor'> & {
  executor: HashSigner;
};

export interface EvnodeSponsorArgs {
  intent: SponsorableIntent;
  sponsor?: HashSigner;
}

export function encodeSignedTransaction(signedTx: EvNodeSignedTransaction): Hex {
  const fields = buildPayloadFields(signedTx.transaction, true);
  const execSig = normalizeSignatureForRlp(signedTx.executorSignature);
  const envelope = toRlp([
    ...fields,
    rlpHexFromBigInt(BigInt(execSig.v)),
    rlpHexFromBigInt(hexToBigInt(execSig.r)),
    rlpHexFromBigInt(hexToBigInt(execSig.s)),
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
  const payload = encodePayloadFieldsNoList(tx, false);
  // Sponsor hash preimage: 0x78 || executor_address (20 bytes) || RLP(field encodings without list header).
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

function isCreateCall(call: Call): boolean {
  return call.to === null;
}

export function estimateIntrinsicGas(calls: Call[]): bigint {
  let gas = BASE_TX_GAS; // base transaction cost

  for (const call of calls) {
    gas += BASE_TX_GAS; // each call costs at least 21000 gas
    if (isCreateCall(call)) gas += CREATE_GAS;

    for (const byte of hexToBytes(call.data)) {
      if (byte === 0) {
        gas += 4n;
      } else {
        gas += 16n;
      }
    }
  }

  return gas;
}

export function validateEvNodeTx(tx: EvNodeTransaction): void {
  if (tx.calls.length === 0) {
    throw new Error('EvNode transaction must include at least one call');
  }

  for (let i = 1; i < tx.calls.length; i += 1) {
    if (isCreateCall(tx.calls[i])) {
      throw new Error('Only the first call may be CREATE');
    }
  }
}

export function evnodeActions(client: Client) {
  return {
    async sendEvNodeTransaction(args: EvnodeSendArgsWithExecutor): Promise<Hex> {
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

    async createSponsorableIntent(args: EvnodeSendArgsWithExecutor): Promise<SponsorableIntent> {
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
    createIntent(args: EvnodeSendArgs): Promise<SponsorableIntent> {
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
      return hexToSignature(signature);
    },
  };
}

function buildPayloadFields(tx: EvNodeTransaction, includeSponsorSig: boolean): RlpValue[] {
  return [
    rlpHexFromBigInt(tx.chainId),
    rlpHexFromBigInt(tx.nonce),
    rlpHexFromBigInt(tx.maxPriorityFeePerGas),
    rlpHexFromBigInt(tx.maxFeePerGas),
    rlpHexFromBigInt(tx.gasLimit),
    encodeCalls(tx.calls),
    encodeAccessList(tx.accessList),
    includeSponsorSig && tx.feePayerSignature
      ? encodeSponsorSignature(tx.feePayerSignature)
      : EMPTY_BYTES,
  ];
}

function encodePayloadFieldsNoList(
  tx: EvNodeTransaction,
  includeSponsorSig: boolean,
): Hex {
  const fields = buildPayloadFields(tx, includeSponsorSig);
  const encodedFields = fields.map((field) => toRlp(field));
  return concat(encodedFields);
}

function encodeCalls(calls: Call[]): RlpValue[] {
  return calls.map((call) => [
    call.to ?? EMPTY_BYTES,
    rlpHexFromBigInt(call.value),
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
  return accessList.map((item) => [item.address, [...item.storageKeys]]);
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

function encodeSponsorSignature(signature: Signature): RlpValue {
  // Encode sponsor signature as 65-byte signature bytes (r || s || v).
  // This matches the common Signature encoding used by alloy primitives.
  if (typeof signature === 'string') {
    return signature;
  }
  const normalized = normalizeSignatureForRlp(signature);
  const vByte = toHex(normalized.v, { size: 1 });
  return concat([normalized.r, normalized.s, vByte]);
}

function decodeSignature(value: RlpValue): Signature | undefined {
  if (value === EMPTY_BYTES) return undefined;

  if (!Array.isArray(value) || value.length !== 3) {
    if (isHex(value)) {
      return signatureFromBytes(value);
    }
    throw new Error('Invalid sponsor signature encoding');
  }

  const [v, r, s] = value;
  return signatureFromParts(v, r, s);
}

function signatureFromBytes(value: Hex): Signature {
  const bytes = hexToBytes(value);
  if (bytes.length !== 65) {
    throw new Error('Invalid sponsor signature length');
  }
  const r = bytesToHex(bytes.slice(0, 32));
  const s = bytesToHex(bytes.slice(32, 64));
  const vRaw = bytes[64];
  const v = vRaw === 27 || vRaw === 28 ? vRaw - 27 : vRaw;
  if (v !== 0 && v !== 1) {
    throw new Error('Invalid signature v value');
  }
  return { yParity: v, v: BigInt(v), r: padTo32Bytes(r), s: padTo32Bytes(s) };
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
    yParity: vNumber,
    v: BigInt(vNumber),
    r: padTo32Bytes(r),
    s: padTo32Bytes(s),
  };
}

function normalizeSignature(signature: Signature): { yParity: number; r: Hex; s: Hex; v?: bigint } {
  const parsed = typeof signature === 'string' ? hexToSignature(signature) : signature;

  const v = Number(parsed.v ?? parsed.yParity);
  const normalizedV = v === 27 || v === 28 ? v - 27 : v;
  if (normalizedV !== 0 && normalizedV !== 1) {
    throw new Error('Invalid signature v value');
  }

  return {
    yParity: normalizedV,
    r: padTo32Bytes(parsed.r),
    s: padTo32Bytes(parsed.s),
  };
}

function normalizeSignatureForRlp(signature: Signature): { v: number; r: Hex; s: Hex } {
  const normalized = normalizeSignature(signature);
  return {
    v: normalized.yParity,
    r: normalized.r,
    s: normalized.s,
  };
}

function padTo32Bytes(value: Hex): Hex {
  return toHex(hexToBigIntSafe(value), { size: 32 });
}

function rlpHexFromBigInt(value: bigint): Hex {
  return value === 0n ? EMPTY_BYTES : toHex(value);
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
