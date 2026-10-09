import { ChildProcess, execFileSync, spawn } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { LIVE } from './live';

/**
 * 偽 HubSpot・偽 Turso・本物のサーバ (debug ビルド) を起動して /health を待つ。teardown で全部止める。
 *   E2E_BIN         サーババイナリ (debug ビルド必須: 偽 Google の discovery 上書きは debug だけ)
 *   E2E_PYTHON      fixture DB を作る python
 */
const ROOT = path.resolve(__dirname, '../../..');
const children: ChildProcess[] = [];

async function waitFor(url: string, ms: number, what: string, tail: () => string): Promise<void> {
  const deadline = Date.now() + ms;
  for (;;) {
    try { const r = await fetch(url); if (r.ok) return; } catch { /* 起動待ち */ }
    if (Date.now() > deadline) throw new Error(`${what} が ${ms / 1000} 秒以内に応答しない: ${url}\n${tail()}`);
    await new Promise((r) => setTimeout(r, 300));
  }
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  const candidates = [
    process.env.E2E_BIN,
    path.join(process.env.CARGO_TARGET_DIR ?? '', 'debug', 'rust_dashboard'),
    path.join(ROOT, 'target-private', 'debug', 'rust_dashboard'),
    path.join(ROOT, 'target', 'debug', 'rust_dashboard'),
  ].filter((p): p is string => typeof p === 'string' && p !== '');
  const bin = candidates.find((p) => fs.existsSync(p));
  if (!bin) throw new Error(`debug ビルドのサーババイナリが無い: ${candidates.join(' / ')}`);

  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hp_crm_write_live_'));
  const logOf = (name: string) => path.join(workDir, name);
  const tail = (name: string) => () => { try { return fs.readFileSync(logOf(name), 'utf8').split('\n').slice(-30).join('\n'); } catch { return ''; } };
  const start = (name: string, cmd: string, args: string[], env: NodeJS.ProcessEnv = process.env): ChildProcess => {
    const fd = fs.openSync(logOf(name), 'w');
    const c = spawn(cmd, args, { cwd: ROOT, stdio: ['ignore', fd, fd], env });
    children.push(c);
    return c;
  };

  start('fake_hubspot.log', process.execPath, [path.join(ROOT, 'scripts/loadtest/fake_hubspot.mjs'),
    '--port', String(LIVE.hubspotPort), '--deals', '20', '--latency-ms', '0', '--jitter-ms', '0', '--writable']);
  start('fake_turso.log', process.execPath, [path.join(ROOT, 'tests/e2e/crm_write_live/fake_turso.mjs'), '--port', String(LIVE.tursoPort)]);
  await waitFor(`${LIVE.hubspot}/_health`, 30_000, '偽 HubSpot', tail('fake_hubspot.log'));
  await waitFor(`${LIVE.turso}/_health`, 30_000, '偽 Turso', tail('fake_turso.log'));

  const db = path.join(workDir, 'hellowork.db');
  const py = process.env.E2E_PYTHON ?? (process.platform === 'win32' ? 'python' : 'python3');
  execFileSync(py, [path.join(ROOT, 'scripts/e2e/make_fixture_db.py'), db], { stdio: 'inherit' });

  const env: NodeJS.ProcessEnv = { ...process.env };
  for (const k of Object.keys(env)) {
    if (/^(GEMINI_|GOOGLE_|OPENAI_|ANTHROPIC_|HUBSPOT_|SLACK_|ZOOM_|CRM_|ADMIN_EMAILS|AUDIT_|.*TURSO.*)/.test(k)) delete env[k];
  }
  const server = start('app.log', bin, [], {
    ...env,
    PORT: String(LIVE.appPort),
    AUTH_PASSWORD: `live-e2e-${process.pid}`,
    ALLOWED_DOMAINS: 'f-a-c.co.jp',
    HELLOWORK_DB_PATH: db,
    HUBSPOT_ACCESS_TOKEN: 'fake-live-e2e-token',
    HUBSPOT_BASE_URL: LIVE.hubspot,
    GOOGLE_OIDC_CLIENT_ID: 'test-client',
    GOOGLE_OIDC_CLIENT_SECRET: 'test-secret',
    GOOGLE_OIDC_REDIRECT_URL: `${LIVE.app}/auth/google/callback`,
    GOOGLE_OIDC_HOSTED_DOMAIN: 'f-a-c.co.jp',
    GOOGLE_OIDC_DISCOVERY_URL_DEBUG: `${LIVE.hubspot}/oidc/.well-known/openid-configuration`,
    // debug ビルドだけが読む CSRF の追加許可 Origin (ブラウザ内 fetch の PATCH を通す)
    CSRF_EXTRA_ORIGINS_DEBUG: LIVE.app,
    AUDIT_TURSO_URL: LIVE.turso,
    AUDIT_TURSO_TOKEN: 'fake-live-e2e-token',
    // 書き込みの栓: この 1 件だけ書ける (もう 1 件は許可リスト外)
    CRM_WRITE_DEAL_ALLOWLIST: LIVE.allowedDealId,
    // 再送 worker を E2E で待てる長さに (debug ビルドだけが読む)
    CRM_WORKER_START_DELAY_SECS_DEBUG: '1',
    CRM_PENDING_BACKOFF_SECS_DEBUG: '2',
    RUST_LOG: process.env.RUST_LOG ?? 'info',
  });
  let exited: number | null | undefined;
  server.on('exit', (code) => { exited = code; });
  await waitFor(`${LIVE.app}/health`, 120_000, 'サーバ', () => (exited === undefined ? tail('app.log')() : `サーバが終了した (code=${exited})\n${tail('app.log')()}`));

  return async () => {
    for (const c of children) { try { c.kill('SIGTERM'); } catch { /* 済 */ } }
    if (!process.env.E2E_KEEP_WORKDIR) {
      await new Promise((r) => setTimeout(r, 500));
      try { fs.rmSync(workDir, { recursive: true, force: true }); } catch { /* 残す */ }
    } else {
      console.log(`workdir: ${workDir}`);
    }
  };
}
