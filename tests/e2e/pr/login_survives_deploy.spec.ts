import { test, expect } from '@playwright/test';
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { startLoginAuditMock } from './helpers/login_audit_mock';
import { E2E_EMAIL, E2E_PASSWORD } from './helpers/fixture_values';

// 外部 API は使わず、同じ鍵で実際の Rust プロセスを入れ替える。
for (const width of [1440, 1920]) {
  test(`幅${width}: 再起動後もログインを保持し、パスワードでGoogle限定画面へ入れない`, async ({ page }) => {
    test.setTimeout(120_000);
    await page.setViewportSize({ width, height: 1000 });
    const root = path.resolve(__dirname, '../../..');
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'hr-login-reboot-'));
    const db = path.join(dir, 'hellowork.db');
    execFileSync(process.env.E2E_PYTHON ?? 'python3', [path.join(root, 'scripts/e2e/make_fixture_db.py'), db]);
    const portFinder = createServer();
    await new Promise<void>((resolve) => portFinder.listen(0, '127.0.0.1', resolve));
    const port = (portFinder.address() as { port: number }).port;
    await new Promise<void>((resolve) => portFinder.close(() => resolve()));
    const base = `http://127.0.0.1:${port}`;
    const env: NodeJS.ProcessEnv = { ...process.env };
    for (const name of Object.keys(env)) {
      if (/^(GEMINI_|GOOGLE_|OPENAI_|ANTHROPIC_|HUBSPOT_|SLACK_|ZOOM_|RENDER|AUTH_|ALLOWED_DOMAINS|SESSION_COOKIE_KEY|.*TURSO.*)/.test(name)) delete env[name];
    }
    const audit = await startLoginAuditMock();
    let server: ChildProcess | undefined;
    const stop = async () => {
      if (server && server.exitCode === null) {
        const exited = once(server, 'exit');
        server.kill('SIGTERM');
        await exited;
      }
    };
    const start = async () => {
      server = spawn(process.env.E2E_BIN ?? path.join(root, '.e2e-bin/rust_dashboard'), [], {
        cwd: root, stdio: ['ignore', 'ignore', 'ignore'],
        env: { ...env, PORT: String(port), AUTH_PASSWORD: E2E_PASSWORD, ALLOWED_DOMAINS: 'f-a-c.co.jp',
          HELLOWORK_DB_PATH: db, AUDIT_TURSO_URL: audit.url, AUDIT_TURSO_TOKEN: 'test-only', SESSION_COOKIE_KEY: Buffer.alloc(64, 42).toString('base64') },
      });
      await expect.poll(async () => {
        if (server?.exitCode !== null) throw new Error('検証用サーバが終了しました');
        try { return (await fetch(`${base}/health`)).status; } catch { return 0; }
      }, { timeout: 60_000 }).toBe(200);
    };
    try {
      await start();
      await page.goto(`${base}/login`);
      await page.fill('#email', E2E_EMAIL);
      await page.fill('#password', E2E_PASSWORD);
      await Promise.all([page.waitForURL((url) => url.pathname === '/'), page.click("form[action='/login'] button[type=submit]")]);
      await expect(page.getByText(`ログイン: ${E2E_EMAIL}`)).toBeVisible();
      const beforeRestart = (await page.context().cookies()).filter((cookie) => ['hr_login', 'id'].includes(cookie.name)).map((cookie) => `${cookie.name}=${cookie.value}`).join('; ');
      const loginCookie = (await page.context().cookies()).find((cookie) => cookie.name === 'hr_login');
      expect(loginCookie?.httpOnly).toBe(true);
      expect(loginCookie?.sameSite).toBe('Lax');
      expect(loginCookie?.expires).toBeGreaterThan(Date.now() / 1000 + 86300);
      await stop();
      await start();
      await page.reload();
      await expect(page.getByText(`ログイン: ${E2E_EMAIL}`)).toBeVisible();
      expect(new URL(page.url()).pathname).toBe('/');
      const crm = await page.request.get(`${base}/api/crm/contacts/1`, { headers: { Accept: 'application/json' } });
      expect(crm.status()).toBe(403);
      expect((await crm.json()).error_kind).toBe('google_login_required');
      const jobCopy = await page.request.get(`${base}/api/job-copy/live`);
      expect(jobCopy.status()).toBe(403);
      const afterRestart = (await page.context().cookies()).filter((cookie) => ['hr_login', 'id'].includes(cookie.name)).map((cookie) => `${cookie.name}=${cookie.value}`).join('; ');
      await page.goto(`${base}/logout`);
      await expect(page.locator('#email')).toBeVisible();
      expect((await page.context().cookies()).some((cookie) => cookie.name === 'hr_login')).toBe(false);
      const denied = await page.request.get(`${base}/api/nav`, { headers: { Accept: 'application/json' } });
      expect(denied.status()).toBe(401);
      // Cookie を削除したブラウザだけでなく、保存済みの値を再送しても認証を復活させない。
      for (const saved of [beforeRestart, afterRestart]) {
        const replay = await page.request.get(`${base}/api/nav`, { headers: { Accept: 'application/json', Cookie: saved } });
        expect(replay.status()).toBe(401);
      }
      await stop();
      await start();
      for (const saved of [beforeRestart, afterRestart]) {
        const replay = await page.request.get(`${base}/api/nav`, { headers: { Accept: 'application/json', Cookie: saved } });
        expect(replay.status()).toBe(401);
      }
    } finally {
      await stop();
      await audit.close();
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
}
