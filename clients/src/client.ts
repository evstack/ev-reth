import {
  type AccessList,
  type Address,
  type Client,
  type Hex,
  hexToBigInt,
  isHex,
} from 'viem';

import type {
  Call,
  EvNodeTransaction,
  EvNodeSignedTransaction,
  SponsorableIntent,
  HashSigner,
  EvnodeClientOptions,
  EvnodeSendArgs,
  EvnodeSendArgsWithExecutor,
  EvnodeSponsorArgs,
} from './types.js';

import {
  encodeSignedTransaction,
  decodeEvNodeTransaction,
  estimateIntrinsicGas,
  validateEvNodeTx,
} from './encoding.js';

import { signAsExecutor, signAsSponsor } from './signing.js';

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

// --- internal helpers ---

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

function hexToBigIntSafe(value: unknown): bigint {
  if (typeof value !== 'string' || !value.startsWith('0x')) {
    throw new Error('Invalid hex value');
  }
  return value === '0x' ? 0n : hexToBigInt(value as `0x${string}`);
}
