import { ChildProcess, execFileSync, spawn } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { E2E_PASSWORD, PR_BASE_URL, PR_PORT, SALES_KPI_FIXTURE } from './helpers/fixture_values';

/**
 * fixture DB を作り、ビルド済みバイナリを起動して /health を待つ。teardown で停止する。
 *   E2E_BIN         バイナリ (既定 .e2e-bin/rust_dashboard[.exe])
 *   E2E_FIXTURE_DB  作成済み fixture を使う (未指定なら make_fixture_db.py で一時パスに作る)
 *   E2E_PYTHON      python 実行ファイル (既定 python / Linux は python3)
 */
const ROOT = path.resolve(__dirname, '../../..');
let server: ChildProcess | undefined;

export default async function globalSetup(): Promise<() => Promise<void>> {
  const exe = process.platform === 'win32' ? 'rust_dashboard.exe' : 'rust_dashboard';
  const bin = process.env.E2E_BIN ?? path.join(ROOT, '.e2e-bin', exe);
  if (!fs.existsSync(bin)) throw new Error(`サーババイナリが無い: ${bin} (E2E_BIN で指定)`);

  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hp_e2e_'));
  let db = process.env.E2E_FIXTURE_DB;
  if (!db) {
    db = path.join(workDir, 'hellowork.db');
    const py = process.env.E2E_PYTHON ?? (process.platform === 'win32' ? 'python' : 'python3');
    execFileSync(py, [path.join(ROOT, 'scripts/e2e/make_fixture_db.py'), db], { stdio: 'inherit' });
  }

  const logPath = path.join(workDir, 'server.log');
  const log = fs.openSync(logPath, 'w');
  // 開発者の環境変数でナビの出し分け (GEMINI_API_KEY → 求人票作成、GOOGLE_ADS_* → キーワード需要) や
  // OIDC が変わらないよう、外部サービス系の変数は子プロセスに渡さない。
  const env: NodeJS.ProcessEnv = { ...process.env };
  for (const k of Object.keys(env)) {
    if (/^(GEMINI_|GOOGLE_|OPENAI_|ANTHROPIC_|HUBSPOT_|SLACK_|ZOOM_|.*TURSO.*)/.test(k)) delete env[k];
  }
  server = spawn(bin, [], {
    cwd: ROOT, // static/ と data/ を相対パスで読むためリポルートで起動する
    stdio: ['ignore', log, log],
    env: {
      ...env,
      PORT: String(PR_PORT),
      AUTH_PASSWORD: E2E_PASSWORD,
      ALLOWED_DOMAINS: 'f-a-c.co.jp',
      HELLOWORK_DB_PATH: db,
      // debug ビルドだけが読む CSRF の追加許可 Origin (src/lib.rs)。ブラウザ内 fetch の POST を通す
      CSRF_EXTRA_ORIGINS_DEBUG: PR_BASE_URL,
      // 営業KPI: Sheets の代わりに TSV から組む経路 (src/handlers/sales_kpi/fixture.rs)。値は helpers/fixture_values.ts の SALES_KPI_FIXTURE
      SALES_KPI_FIXTURE_DIR: path.join(ROOT, 'tests/fixtures/sales_kpi'),
      SALES_KPI_FIXTURE_TODAY: SALES_KPI_FIXTURE.today,
    },
  });
  let exited: number | null | undefined;
  server.on('exit', (code) => { exited = code; });

  const deadline = Date.now() + 90_000;
  for (;;) {
    if (exited !== undefined) throw new Error(`サーバが終了した (code=${exited})\n${tail(logPath)}`);
    try {
      const r = await fetch(`http://localhost:${PR_PORT}/health`);
      if (r.ok) break;
    } catch { /* 起動待ち */ }
    if (Date.now() > deadline) throw new Error(`/health が 90 秒以内に応答しない\n${tail(logPath)}`);
    await new Promise((r) => setTimeout(r, 500));
  }

  return async () => {
    if (server?.pid && exited === undefined) {
      if (process.platform === 'win32') {
        try { execFileSync('taskkill', ['/PID', String(server.pid), '/T', '/F'], { stdio: 'ignore' }); } catch { /* 済 */ }
      } else {
        server.kill('SIGTERM');
      }
    }
    if (!process.env.E2E_KEEP_WORKDIR) {
      await new Promise((r) => setTimeout(r, 500));
      try { fs.rmSync(workDir, { recursive: true, force: true }); } catch { /* ロック中は残す */ }
    }
  };
}

function tail(p: string): string {
  try { return fs.readFileSync(p, 'utf8').split('\n').slice(-30).join('\n'); } catch { return ''; }
}
