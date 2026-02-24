import type { Address, Hex } from 'viem';
import { hexToSignature } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import {
  signAsSponsor,
  encodeSignedTransaction,
  type SponsorableIntent,
  type HashSigner,
  type EvNodeSignedTransaction,
} from '@evstack/evnode-viem';
import type { SponsorConfig } from '../config.js';
import { PolicyEngine } from './policy-engine.js';
import { SPONSOR_BALANCE_LOW, NODE_ERROR } from '../errors.js';

export interface SponsorResult {
  txHash: Hex;
  sponsorAddress: Address;
}

export class SponsorService {
  public readonly policyEngine: PolicyEngine;
  private readonly sponsorSigner: HashSigner;
  private readonly rpcUrl: string;
  private readonly minBalance: bigint;

  constructor(config: SponsorConfig) {
    this.policyEngine = new PolicyEngine(config);
    this.rpcUrl = config.rpcUrl;
    this.minBalance = config.minSponsorBalance;

    const account = privateKeyToAccount(config.sponsorPrivateKey);
    this.sponsorSigner = {
      address: account.address,
      signHash: async (hash: Hex) => {
        const sig = await account.sign({ hash });
        return hexToSignature(sig);
      },
    };
  }

  get sponsorAddress(): Address {
    return this.sponsorSigner.address;
  }

  async sponsorIntent(intent: SponsorableIntent): Promise<SponsorResult> {
    await this.policyEngine.validate(intent);

    const balance = await this.getSponsorBalance();
    if (balance < this.minBalance) {
      throw SPONSOR_BALANCE_LOW();
    }

    const feePayerSignature = await signAsSponsor(
      intent.tx,
      intent.executorAddress,
      this.sponsorSigner,
    );

    const signedTx: EvNodeSignedTransaction = {
      transaction: { ...intent.tx, feePayerSignature },
      executorSignature: intent.executorSignature,
    };
    const encoded = encodeSignedTransaction(signedTx);
    const txHash = await this.sendRawTransaction(encoded);

    return { txHash, sponsorAddress: this.sponsorSigner.address };
  }

  private async fetchRpc(body: unknown): Promise<Response> {
    return fetch(this.rpcUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  }

  private async rpcCall(method: string, params: unknown[] = []): Promise<any> {
    const response = await this.fetchRpc({ jsonrpc: '2.0', id: 1, method, params });
    const data = (await response.json()) as { result?: any; error?: { message: string } };
    if (data.error) throw NODE_ERROR(data.error.message);
    return data.result;
  }

  async getSponsorBalance(): Promise<bigint> {
    const result = await this.rpcCall('eth_getBalance', [this.sponsorSigner.address, 'latest']);
    return BigInt(result);
  }

  async isNodeConnected(): Promise<boolean> {
    try {
      const result = await this.rpcCall('net_version');
      return !!result;
    } catch {
      return false;
    }
  }

  async sendRawTransaction(encoded: Hex): Promise<Hex> {
    return this.rpcCall('eth_sendRawTransaction', [encoded]);
  }

  async proxyRpcRequest(body: unknown): Promise<unknown> {
    const response = await this.fetchRpc(body);
    return response.json();
  }
}
