import type { FastifyInstance } from 'fastify';
import type { SponsorService } from '../services/sponsor-service.js';

export function registerHealthRoute(app: FastifyInstance, service: SponsorService) {
  app.get('/v1/health', async (_request, reply) => {
    const [balance, nodeConnected] = await Promise.allSettled([
      service.rpc.getBalance(service.sponsorAddress),
      service.rpc.isConnected(),
    ]);

    const sponsorBalance = balance.status === 'fulfilled' ? balance.value.toString() : null;
    const connected = nodeConnected.status === 'fulfilled' ? nodeConnected.value : false;

    let status: 'healthy' | 'degraded' | 'unhealthy';
    if (!connected) {
      status = 'unhealthy';
    } else if (sponsorBalance === null) {
      status = 'degraded';
    } else {
      status = 'healthy';
    }

    const httpStatus = status === 'unhealthy' ? 503 : 200;
    return reply.status(httpStatus).send({ status, sponsorBalance, nodeConnected: connected });
  });
}
