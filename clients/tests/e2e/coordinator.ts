import { createHmac, randomBytes } from 'node:crypto';
import { keccak256, type Hex } from 'viem';

export interface CoordinatorOptions {
  rpcUrl: string;
  engineUrl: string;
  jwtSecret: string; // hex-encoded 32 bytes (no 0x prefix)
  pollIntervalMs?: number;
  feeRecipient?: Hex;
  gasLimit?: number;
}

export class Coordinator {
  private rpcUrl: string;
  private engineUrl: string;
  private jwtSecret: Buffer;
  private pollIntervalMs: number;
  private feeRecipient: Hex;
  private gasLimit: number;

  private parentHash: Hex = '0x0000000000000000000000000000000000000000000000000000000000000000';
  private blockNumber: bigint = 0n;
  private timestamp: bigint = 0n;

  private running = false;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;
  private seenTxs = new Set<string>();

  constructor(opts: CoordinatorOptions) {
    this.rpcUrl = opts.rpcUrl;
    this.engineUrl = opts.engineUrl;
    this.jwtSecret = Buffer.from(opts.jwtSecret, 'hex');
    this.pollIntervalMs = opts.pollIntervalMs ?? 200;
    this.feeRecipient = opts.feeRecipient ?? '0x0000000000000000000000000000000000000000';
    this.gasLimit = opts.gasLimit ?? 30_000_000;
  }

  async start(): Promise<void> {
    const latestBlock = await this.rpcCall(this.rpcUrl, 'eth_getBlockByNumber', ['latest', false]);
    this.parentHash = latestBlock.hash;
    this.blockNumber = BigInt(latestBlock.number);
    this.timestamp = BigInt(latestBlock.timestamp);
    this.running = true;
    this.poll();
  }

  stop(): void {
    this.running = false;
    if (this.pollTimer) {
      clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }
  }

  getBlockNumber(): bigint {
    return this.blockNumber;
  }

  private poll(): void {
    if (!this.running) return;

    this.pollOnce()
      .catch((err) => {
        console.error('[coordinator] poll error:', err);
      })
      .finally(() => {
        if (this.running) {
          this.pollTimer = setTimeout(() => this.poll(), this.pollIntervalMs);
        }
      });
  }

  private async pollOnce(): Promise<void> {
    const rawTxs: Hex[] = await this.rpcCall(this.rpcUrl, 'txpoolExt_getTxs', []);

    const newTxs: Hex[] = [];
    for (const rawTx of rawTxs) {
      const txHash = keccak256(rawTx);
      if (!this.seenTxs.has(txHash)) {
        this.seenTxs.add(txHash);
        newTxs.push(rawTx);
      }
    }

    if (newTxs.length > 0) {
      await this.mineBlock(newTxs);
    }
  }

  private async mineBlock(txs: Hex[]): Promise<void> {
    const newTimestamp = this.timestamp + 12n;
    const prevRandao = '0x' + randomBytes(32).toString('hex') as Hex;

    const forkchoiceState = {
      headBlockHash: this.parentHash,
      safeBlockHash: this.parentHash,
      finalizedBlockHash: this.parentHash,
    };

    const payloadAttributes = {
      timestamp: '0x' + newTimestamp.toString(16),
      prevRandao,
      suggestedFeeRecipient: this.feeRecipient,
      withdrawals: [],
      parentBeaconBlockRoot: '0x0000000000000000000000000000000000000000000000000000000000000000',
      transactions: txs,
      gasLimit: this.gasLimit,
    };

    // Step 1: FCU with payload attributes -> get payloadId
    const fcuResult = await this.engineCall('engine_forkchoiceUpdatedV3', [
      forkchoiceState,
      payloadAttributes,
    ]);
    const payloadId = fcuResult.payloadId;
    if (!payloadId) {
      throw new Error('No payloadId returned from forkchoiceUpdated');
    }

    // Step 2: getPayload -> get execution payload
    const payloadEnvelope = await this.engineCall('engine_getPayloadV3', [payloadId]);
    const executionPayload = payloadEnvelope.executionPayload;

    // Step 3: newPayload -> validate
    const newPayloadStatus = await this.engineCall('engine_newPayloadV3', [
      executionPayload,
      [],
      '0x0000000000000000000000000000000000000000000000000000000000000000',
    ]);
    if (newPayloadStatus.status !== 'VALID') {
      throw new Error(`newPayload returned status: ${newPayloadStatus.status}`);
    }

    // Step 4: FCU to finalize new head
    const newBlockHash = executionPayload.blockHash;
    await this.engineCall('engine_forkchoiceUpdatedV3', [
      {
        headBlockHash: newBlockHash,
        safeBlockHash: newBlockHash,
        finalizedBlockHash: newBlockHash,
      },
      null,
    ]);

    // Update internal state
    this.parentHash = newBlockHash;
    this.blockNumber = BigInt(executionPayload.blockNumber);
    this.timestamp = BigInt(executionPayload.timestamp);
  }

  private async rpcCall(url: string, method: string, params: unknown[]): Promise<any> {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    const json = await res.json();
    if (json.error) {
      throw new Error(`RPC ${method}: ${json.error.message ?? JSON.stringify(json.error)}`);
    }
    return json.result;
  }

  private async engineCall(method: string, params: unknown[]): Promise<any> {
    const token = this.createJwt();
    const res = await fetch(this.engineUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
    });
    const json = await res.json();
    if (json.error) {
      throw new Error(`Engine ${method}: ${json.error.message ?? JSON.stringify(json.error)}`);
    }
    return json.result;
  }

  private createJwt(): string {
    const header = { alg: 'HS256', typ: 'JWT' };
    const now = Math.floor(Date.now() / 1000);
    const payload = { iat: now, exp: now + 3600 };

    const b64Header = base64url(JSON.stringify(header));
    const b64Payload = base64url(JSON.stringify(payload));
    const unsigned = `${b64Header}.${b64Payload}`;

    const signature = createHmac('sha256', this.jwtSecret)
      .update(unsigned)
      .digest();

    return `${unsigned}.${base64url(signature)}`;
  }
}

function base64url(input: string | Buffer): string {
  const buf = typeof input === 'string' ? Buffer.from(input) : input;
  return buf.toString('base64url');
}
