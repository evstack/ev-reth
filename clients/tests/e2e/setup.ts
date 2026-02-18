import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { randomBytes } from 'node:crypto';
import { Coordinator } from './coordinator.ts';

const RPC_PORT = 8545;
const ENGINE_PORT = 8551;

export interface TestContext {
  rpcUrl: string;
  coordinator: Coordinator;
  cleanup: () => Promise<void>;
}

export async function setupTestNode(): Promise<TestContext> {
  const repoRoot = resolve(import.meta.dirname, '..', '..', '..');

  // 1. Create temp dir
  const tmpDir = await mkdtemp(join(tmpdir(), 'ev-reth-e2e-'));

  // 2. Generate JWT secret
  const jwtSecret = randomBytes(32).toString('hex');
  const jwtPath = join(tmpDir, 'jwt.hex');
  await writeFile(jwtPath, jwtSecret);

  // 3. Find ev-reth binary
  const releaseBin = join(repoRoot, 'target', 'release', 'ev-reth');
  const debugBin = join(repoRoot, 'target', 'debug', 'ev-reth');
  let binaryPath: string;
  if (existsSync(releaseBin)) {
    binaryPath = releaseBin;
  } else if (existsSync(debugBin)) {
    binaryPath = debugBin;
  } else {
    throw new Error(
      `ev-reth binary not found. Run 'make build' or 'make build-dev' first.\n` +
        `Checked: ${releaseBin}\n         ${debugBin}`,
    );
  }

  // 4. Spawn ev-reth
  const genesisPath = join(repoRoot, 'crates', 'tests', 'assets', 'genesis.json');
  const dataDir = join(tmpDir, 'data');

  const child = spawn(binaryPath, [
    'node',
    '--chain', genesisPath,
    '--datadir', dataDir,
    '--http',
    '--http.port', String(RPC_PORT),
    '--http.api', 'eth,net,web3',
    '--authrpc.port', String(ENGINE_PORT),
    '--authrpc.jwtsecret', jwtPath,
    '--log.stdout.filter', 'error',
  ], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  // Forward stderr for debugging if needed
  child.stderr?.on('data', (data: Buffer) => {
    const msg = data.toString().trim();
    if (msg) console.error('[ev-reth]', msg);
  });

  // 5. Wait for RPC to be ready
  const rpcUrl = `http://127.0.0.1:${RPC_PORT}`;
  await waitForRpc(rpcUrl, child);

  // 6. Create and start coordinator
  const engineUrl = `http://127.0.0.1:${ENGINE_PORT}`;
  const coordinator = new Coordinator({
    rpcUrl,
    engineUrl,
    jwtSecret,
    pollIntervalMs: 200,
  });
  await coordinator.start();

  // 7. Return context
  const cleanup = async () => {
    coordinator.stop();
    await killProcess(child);
    await rm(tmpDir, { recursive: true, force: true }).catch(() => {});
  };

  return { rpcUrl, coordinator, cleanup };
}

async function waitForRpc(rpcUrl: string, child: ChildProcess): Promise<void> {
  const timeoutMs = 30_000;
  const intervalMs = 500;
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    // Check if process died
    if (child.exitCode !== null) {
      throw new Error(`ev-reth exited with code ${child.exitCode} before RPC was ready`);
    }

    try {
      const res = await fetch(rpcUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'eth_chainId', params: [] }),
      });
      const json = await res.json();
      if (json.result) return;
    } catch {
      // not ready yet
    }

    await new Promise((r) => setTimeout(r, intervalMs));
  }

  throw new Error(`ev-reth RPC did not become ready within ${timeoutMs}ms`);
}

async function killProcess(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null) return;

  child.kill('SIGTERM');

  const exited = await Promise.race([
    new Promise<boolean>((resolve) => child.on('exit', () => resolve(true))),
    new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 5_000)),
  ]);

  if (!exited && child.exitCode === null) {
    child.kill('SIGKILL');
    await new Promise<void>((resolve) => child.on('exit', () => resolve()));
  }
}
