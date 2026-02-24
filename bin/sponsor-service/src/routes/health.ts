import type { FastifyInstance } from 'fastify';
import type { SponsorService } from '../services/sponsor-service.js';

export function registerHealthRoute(app: FastifyInstance, service: SponsorService) {
  app.get('/v1/health', async (_request, reply) => {
    const [balance, nodeConnected] = await Promise.allSettled([
      service.getSponsorBalance(),
      service.isNodeConnected(),
    ]);

    const sponsorBalance = balance.status === 'fulfilled' ? balance.value.toString() : null;
    const connected = nodeConnected.status === 'fulfilled' ? nodeConnected.value : false;
    const status = sponsorBalance !== null && connected ? 'healthy' : 'degraded';

    return reply.send({ status, sponsorBalance, nodeConnected: connected });
  });
}
