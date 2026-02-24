import type { FastifyInstance } from 'fastify';
import type { Hex } from 'viem';
import { decodeEvNodeTransaction, recoverExecutor, EVNODE_TX_TYPE } from '@evstack/evnode-viem';
import type { SponsorService } from '../services/sponsor-service.js';
import { SponsorError } from '../errors.js';

interface JsonRpcRequest {
  jsonrpc: string;
  id: number | string | null;
  method: string;
  params?: any[];
}

function jsonRpcOk(id: number | string | null, result: unknown) {
  return { jsonrpc: '2.0', id, result };
}

function jsonRpcError(id: number | string | null, code: number, message: string) {
  return { jsonrpc: '2.0', id, error: { code, message } };
}

const EVNODE_TX_TYPE_HEX = EVNODE_TX_TYPE.toString(16);

function isEvNodeTx(rawTx: Hex): boolean {
  return rawTx.length >= 4 && rawTx.slice(2, 4).toLowerCase() === EVNODE_TX_TYPE_HEX;
}

function sponsorErrorToRpcCode(e: SponsorError): number {
  switch (e.statusCode) {
    case 429: return -32005;
    case 503: return -32003;
    case 502: return -32003;
    default: return -32602;
  }
}

async function handleEvNodeSendRaw(rawTx: Hex, service: SponsorService): Promise<Hex> {
  const signedTx = decodeEvNodeTransaction(rawTx);

  // Already has sponsor signature — forward as-is
  if (signedTx.transaction.feePayerSignature) {
    return service.sendRawTransaction(rawTx);
  }

  // No sponsor signature — sponsor it
  const executorAddress = await recoverExecutor(signedTx);
  const result = await service.sponsorIntent({
    tx: signedTx.transaction,
    executorSignature: signedTx.executorSignature,
    executorAddress,
  });
  return result.txHash;
}

export function registerJsonRpcRoute(app: FastifyInstance, service: SponsorService) {
  app.post('/', async (request, reply) => {
    const body = request.body as JsonRpcRequest;

    if (body.method === 'eth_sendRawTransaction' && body.params?.[0]) {
      const rawTx = body.params[0] as Hex;

      if (isEvNodeTx(rawTx)) {
        try {
          const txHash = await handleEvNodeSendRaw(rawTx, service);
          return reply.send(jsonRpcOk(body.id, txHash));
        } catch (e) {
          if (e instanceof SponsorError) {
            return reply.send(jsonRpcError(body.id, sponsorErrorToRpcCode(e), e.message));
          }
          return reply.send(jsonRpcError(body.id, -32000, (e as Error).message));
        }
      }
    }

    // Everything else: proxy to the real node
    try {
      const proxyResult = await service.proxyRpcRequest(body);
      return reply.send(proxyResult);
    } catch {
      return reply.send(jsonRpcError(body.id, -32003, 'Unable to connect to upstream node'));
    }
  });
}
