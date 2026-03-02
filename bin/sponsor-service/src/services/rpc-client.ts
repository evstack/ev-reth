import type { Hex } from 'viem';
import { NODE_ERROR } from '../errors.js';

export class RpcClient {
  private nextId = 1;

  constructor(private readonly rpcUrl: string) {}

  private async fetchRpc(body: unknown, timeoutMs = 10_000): Promise<Response> {
    return fetch(this.rpcUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
  }

  async call(method: string, params: unknown[] = []): Promise<any> {
    const id = this.nextId++;
    const response = await this.fetchRpc({ jsonrpc: '2.0', id, method, params });
    const data = (await response.json()) as { result?: any; error?: { message: string } };
    if (data.error) throw NODE_ERROR(data.error.message);
    return data.result;
  }

  async getBalance(address: string): Promise<bigint> {
    const result = await this.call('eth_getBalance', [address, 'latest']);
    return BigInt(result);
  }

  async isConnected(): Promise<boolean> {
    try {
      const result = await this.call('net_version');
      return !!result;
    } catch {
      return false;
    }
  }

  async sendRawTransaction(encoded: Hex): Promise<Hex> {
    return this.call('eth_sendRawTransaction', [encoded]);
  }

  async proxy(body: unknown): Promise<unknown> {
    const response = await this.fetchRpc(body);
    try {
      return await response.json();
    } catch {
      throw NODE_ERROR(`upstream returned non-JSON response (status ${response.status})`);
    }
  }
}
