/**
 * tests/e2e/helpers/session.ts
 *
 * Phase 3 Step 5 Phase 7 (Worker P7): MarketIntelligence E2E 共通 helper
 *
 * 既存 spec (regression_2026_04_26.spec.ts / survey_deepdive_2026_04_26.spec.ts)
 * の login / clickNavTab / uploadCsv ロジックを抽出・再利用しつつ、
 * 「アップロード後に /report/survey?session_id=... の session_id を取得する」
 * 機能を追加した。
 *
 * 設計指針:
 *   - .env を直接 open しない (環境変数経由)
 *   - 認証情報を console.log / snapshot に出さない
 *   - 既存 spec を変更しない (新規 helper として追加)
 *
 * 環境変数要件:
 *   BASE_URL    既定: http://localhost:9216 (playwright.config.ts と整合)
 *   E2E_EMAIL   ログイン Email (PowerShell `$env:E2E_EMAIL` で設定)
 *   E2E_PASS    ログイン Password
 *
 *   ※ 旧仕様の AUTH_PASSWORD は使用しない。プロジェクト既存 spec が
 *     E2E_EMAIL / E2E_PASS を採用しているため、それに合わせる。
 */

import { Page, expect } from '@playwright/test';
import * as fs from 'fs';

const EMAIL = process.env.E2E_EMAIL ?? '';
const PASSWORD = process.env.E2E_PASS ?? '';

/**
 * 認証情報が設定されていることを確認する。
 * 設定されていない場合は明示的に throw して原因を分かりやすくする。
 */
export function ensureCredentials(): { email: string; password: string } {
  if (!EMAIL || !PASSWORD) {
    throw new Error(
      'E2E_EMAIL / E2E_PASS env vars not set. ' +
        'Set them in PowerShell before running:\n' +
        '  $env:E2E_EMAIL = "..."\n' +
        '  $env:E2E_PASS = "..."',
    );
  }
  return { email: EMAIL, password: PASSWORD };
}

/**
 * ログインしてダッシュボード初期表示まで待つ。
 *
 * 既にログイン済みの場合は再ログインしない。
 * Render cold start を考慮して長めの timeout を設定。
 */
export async function login(page: Page, baseUrl?: string): Promise<void> {
  const { email, password } = ensureCredentials();
  const url = baseUrl ?? process.env.BASE_URL ?? 'http://localhost:9216';

  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 120_000 });
  if (page.url().includes('/login')) {
    await page.fill('input[name="email"]', email);
    await page.fill('input[name="password"]', password);
    await Promise.all([
      page.waitForURL((u) => !u.toString().includes('/login'), { timeout: 120_000 }),
      page.click('button[type="submit"]'),
    ]);
  }
  // ダッシュボード初期描画を待つ
  await page.waitForSelector('.tab-btn', { timeout: 90_000 });
  await page.waitForFunction(
    () => {
      const el = document.querySelector('#content');
      return !!el && (el as HTMLElement).innerHTML.length > 1000;
    },
    null,
    { timeout: 120_000 },
  );
}

/**
 * 上位ナビボタンをクリックして HTMX 差替完了を待つ。
 * 同タブ再クリックは no-op になるため active を検知して skip する。
 *
 * regression_2026_04_26.spec.ts と同等のロジック。
 */
export async function clickNavTab(
  page: Page,
  label: string,
  expectedText?: string,
): Promise<void> {
  const btn = page.locator(`.tab-btn:has-text("${label}")`).first();
  await expect(btn).toBeVisible({ timeout: 30_000 });

  const alreadyActive = await btn.evaluate((el) => el.classList.contains('active'));
  if (alreadyActive) {
    await page.waitForFunction(
      () => {
        const el = document.querySelector('#content') as HTMLElement | null;
        return !!el && el.innerHTML.length > 500;
      },
      null,
      { timeout: 30_000 },
    );
    return;
  }

  await page.evaluate(() => {
    (window as any).__e2eSwapDone = false;
    if ((window as any).__e2eSwapListener) {
      document.removeEventListener('htmx:afterSwap', (window as any).__e2eSwapListener);
    }
    const handler = (ev: Event) => {
      const detail = (ev as CustomEvent).detail;
      if (detail?.target?.id === 'content') {
        (window as any).__e2eSwapDone = true;
      }
    };
    (window as any).__e2eSwapListener = handler;
    document.addEventListener('htmx:afterSwap', handler);
  });

  await btn.click();
  await expect(btn).toHaveClass(/active/, { timeout: 30_000 });

  await page.waitForFunction(
    (txt: string | null) => {
      if (!(window as any).__e2eSwapDone) return false;
      const el = document.querySelector('#content') as HTMLElement | null;
      if (!el || el.innerHTML.length < 500) return false;
      if (txt && !el.innerText.includes(txt)) return false;
      return true;
    },
    expectedText ?? null,
    { timeout: 60_000 },
  );

  await page.waitForTimeout(800);
}

/**
 * 媒体分析タブで CSV をアップロードし、結果が表示されるまで待つ。
 */
export async function uploadCsv(
  page: Page,
  csvPath: string,
  sourceType: 'indeed' | 'jobbox' | 'other' | 'auto' = 'indeed',
  wageMode: 'monthly' | 'hourly' | 'auto' = 'monthly',
): Promise<void> {
  if (!fs.existsSync(csvPath)) {
    throw new Error(`Fixture CSV not found: ${csvPath}`);
  }

  await clickNavTab(page, '媒体分析');

  await page.locator('select#source-type').waitFor({ state: 'visible', timeout: 30_000 });
  await page.selectOption('select#source-type', sourceType);
  await page.selectOption('select#wage-mode', wageMode);

  const uploadResponsePromise = page.waitForResponse(
    (r) => r.url().includes('/api/survey/upload') && r.request().method() === 'POST',
    { timeout: 120_000 },
  );

  const fileInput = page.locator('input[type="file"][name="csv_file"]');
  await fileInput.setInputFiles(csvPath);

  const uploadRes = await uploadResponsePromise;
  expect(
    uploadRes.status(),
    `upload status=${uploadRes.status()} (期待値 < 400)`,
  ).toBeLessThan(400);

  await page.waitForFunction(
    () => {
      const el = document.querySelector('#survey-result') as HTMLElement | null;
      return !!el && el.innerHTML.length > 1000;
    },
    null,
    { timeout: 60_000 },
  );
  await page.waitForTimeout(1500);
}

/**
 * アップロード結果に含まれる `/report/survey?session_id=...` から session_id を取り出す。
 *
 * UI 実装 (templates 上): アップロード成功時、結果 HTML に
 *   `<a href="/report/survey?session_id=s_xxxx">レポート表示</a>`
 * のようなリンクが含まれる。これを抽出する。
 *
 * 取れない場合は throw する (空 session_id を返してから 500 になるのを防ぐ)。
 */
export async function extractSessionId(page: Page): Promise<string> {
  const sessionId = await page.evaluate(() => {
    const links = Array.from(
      document.querySelectorAll('a[href*="/report/survey"]'),
    ) as HTMLAnchorElement[];
    for (const link of links) {
      const href = link.getAttribute('href') ?? '';
      const match = href.match(/session_id=([A-Za-z0-9_-]+)/);
      if (match) return match[1];
    }
    return null;
  });
  if (!sessionId) {
    throw new Error(
      'session_id を抽出できませんでした (#survey-result 内に /report/survey?session_id=... のリンクが無い)',
    );
  }
  return sessionId;
}

/**
 * login → uploadCsv → session_id 抽出 を一括で実行する高水準 helper。
 *
 * 4 つの spec すべてが先頭でこの 3 ステップを必要とするため共通化。
 */
export async function loginAndUpload(
  page: Page,
  csvPath: string,
  sourceType: 'indeed' | 'jobbox' | 'other' | 'auto' = 'indeed',
): Promise<{ sessionId: string }> {
  await login(page);
  await uploadCsv(page, csvPath, sourceType, 'monthly');
  const sessionId = await extractSessionId(page);
  return { sessionId };
}

/**
 * /report/survey?session_id=<id>&variant=<v>[&theme=<t>] を構築。
 */
export function buildReportUrl(
  sessionId: string,
  variant?: 'full' | 'public' | 'market_intelligence',
  theme?: string,
): string {
  const params = new URLSearchParams();
  params.set('session_id', sessionId);
  if (variant) params.set('variant', variant);
  if (theme) params.set('theme', theme);
  return `/report/survey?${params.toString()}`;
}
