export class SponsorError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly statusCode: number = 400,
  ) {
    super(message);
    this.name = 'SponsorError';
  }
}

export const INVALID_INTENT = (detail: string) =>
  new SponsorError('INVALID_INTENT', `Invalid intent: ${detail}`, 400);

export const INVALID_EXECUTOR_SIGNATURE = () =>
  new SponsorError('INVALID_EXECUTOR_SIGNATURE', 'Executor signature does not match declared executor address', 400);

export const CHAIN_ID_MISMATCH = (expected: bigint, got: bigint) =>
  new SponsorError('CHAIN_ID_MISMATCH', `Chain ID mismatch: expected ${expected}, got ${got}`, 400);

export const GAS_LIMIT_EXCEEDED = (max: bigint, got: bigint) =>
  new SponsorError('GAS_LIMIT_EXCEEDED', `Gas limit ${got} exceeds max ${max}`, 400);

export const FEE_TOO_HIGH = (max: bigint, got: bigint) =>
  new SponsorError('FEE_TOO_HIGH', `Max fee per gas ${got} exceeds limit ${max}`, 400);

export const SPONSOR_BALANCE_LOW = () =>
  new SponsorError('SPONSOR_BALANCE_LOW', 'Sponsor balance too low to guarantee gas payment', 503);

export const NODE_ERROR = (detail: string) =>
  new SponsorError('NODE_ERROR', `Node error: ${detail}`, 502);
