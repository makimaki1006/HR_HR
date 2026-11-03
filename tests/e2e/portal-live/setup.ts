import { spawn } from 'node:child_process';
import { closeSync, existsSync, mkdtempSync, openSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { request } from '@playwright/test';

const ROOT = resolve(__dirname, '../../..');
const BASE = 'http://localhost:9230';
const PASSWORD = 'Portal-local-test-2026';

export default async function setup() {
  const bin = process.env.PORTAL_E2E_BIN ?? join(ROOT, 'target/debug/rust_dashboard');
  if (!existsSync(bin)) throw new Error('先に cargo build --bin rust_dashboard と frontend の build を実行してください');
  // .env の探索先も外部の鍵も本番から分離。メモリの保存先・架空シート・送らないメールだけを使う。
  const work = mkdtempSync(join(tmpdir(), 'hr-hr-portal-e2e-'));
  writeFileSync(join(work, '.env'), '');
  symlinkSync(join(ROOT, 'static'), join(work, 'static'), 'dir');
  const logPath = join(work, 'server.log');
  const log = openSync(logPath, 'w');
  const server = spawn(bin, [], {
    cwd: work, stdio: ['ignore', log, log],
    env: {
      PATH: process.env.PATH, TMPDIR: process.env.TMPDIR,
      PORT: '9230', PORTAL_DEV_MEMORY: '1', RUST_LOG: 'info',
      AUTH_PASSWORD: 'Local-server-test-only', HELLOWORK_DB_ENABLED: '0',
      CSRF_EXTRA_ORIGINS_DEBUG: BASE,
    },
  });
  closeSync(log);
  let exited = false;
  server.on('exit', () => { exited = true; });
  const logs = () => readFileSync(logPath, 'utf8');
  try {
    const deadline = Date.now() + 60_000;
    while (true) {
      if (exited) throw new Error(`開発用サーバーが終了しました。ログ: ${logPath}`);
      try { if ((await fetch(`${BASE}/health`)).ok) break; } catch { /* 起動待ち */ }
      if (Date.now() > deadline) throw new Error(`起動を確認できません。ログ: ${logPath}`);
      await new Promise(r => setTimeout(r, 250));
    }
    const token = /\/portal\/invite#t=([A-Za-z0-9_-]+)/.exec(logs())?.[1];
    if (!token) throw new Error('架空アカウントの招待がありません');
    const api = await request.newContext({ baseURL: BASE, extraHTTPHeaders: { Origin: BASE, 'X-Requested-With': 'fetch' } });
    const accepted = await api.post('/api/portal/invite/accept', { data: { token, password: PASSWORD } });
    if (!accepted.ok()) throw new Error(`架空アカウントを作れません: ${accepted.status()}`);
    const login = await api.post('/api/portal/login', { data: { email: 'dev@example.com', password: PASSWORD } });
    if (!login.ok()) throw new Error(`開発用ログインが失敗しました: ${login.status()}`);
    // DevLogMailer がログにだけ出す確認コード。Gmail は呼ばない。
    const code = /\[portal dev\] 確認コード (\d{6})/.exec(logs())?.[1];
    if (!code) throw new Error('送信しない確認コードがありません');
    const verified = await api.post('/api/portal/login/code', { data: { code, remember: true } });
    if (!verified.ok()) throw new Error(`確認コードでログインできません: ${verified.status()}`);
    const state = join(work, 'storage.json');
    await api.storageState({ path: state });
    await api.dispose();
    process.env.PORTAL_E2E_STATE = state;
    process.env.PORTAL_E2E_WORK = work;
  } catch (error) {
    server.kill('SIGTERM');
    throw error;
  }
  return async () => {
    server.kill('SIGTERM');
    // 失敗時の調査用に、架空の値だけのログと状態を一時フォルダへ残す。
  };
}
