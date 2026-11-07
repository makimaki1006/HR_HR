// コンサルKPI v2 (/app/consulting-v2) の E2E。API は架空データ (frontend/src/screens/consulting-v2/__fixtures__) で差し替える。
// 実行: BASE_URL=http://localhost:9217 E2E_EMAIL=... E2E_PASS=... で起動済みのサーバに対して
//   (ローカルで AUTH_PASSWORD を設定して起動し、同じ値を E2E_PASS に渡す)。
// スクリーンショット: CV2_SHOTS=出力先ディレクトリ を渡したときだけ撮る (1440 幅)。
import { test, expect, type Page } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { DATA_QUALITY, STAGE_STAY, FOCUS, BOARD, CUSTOMER, CUSTOMER_INDEX, DETAIL, GOALS, MTGQ, PHONE, RENEWAL_PIPE, RESULTS, TEAM, TODAY } from '../../frontend/src/screens/consulting-v2/__fixtures__/fixtures';

const email = process.env.E2E_EMAIL ?? 'tester@example.invalid';
const pass = process.env.E2E_PASS ?? '';

async function login(page: Page) {
  await page.goto('/login');
  await page.fill('#email', email);
  await page.fill('#password', pass);
  await page.click('button[type=submit]');
  await page.waitForURL((u) => !u.pathname.startsWith('/login'));
}

async function mockApi(page: Page) {
  await page.route('**/api/consulting/data-quality*', (r) => r.fulfill({ json: DATA_QUALITY }));
  await page.route('**/api/consulting/stage-stay*', (r) => r.fulfill({ json: STAGE_STAY }));
  await page.route('**/api/consulting/focus*', (r) => r.fulfill({ json: FOCUS }));
  await page.route('**/api/consulting/team*', (r) => r.fulfill({ json: TEAM }));
  await page.route('**/api/consulting/today*', (r) => r.fulfill({ json: TODAY }));
  await page.route('**/api/consulting/deals*', (r) => r.fulfill({ json: BOARD }));
  await page.route('**/api/consulting/deal-detail*', (r) => r.fulfill({ json: DETAIL }));
  await page.route('**/api/consulting/renewal-pipe*', (r) => r.fulfill({ json: RENEWAL_PIPE }));
  await page.route('**/api/consulting/customer*', (r) => r.fulfill({ json: r.request().url().includes('houjin=') ? CUSTOMER : CUSTOMER_INDEX }));
  await page.route('**/api/consulting/results*', (r) => r.fulfill({ json: RESULTS }));
  await page.route('**/api/consulting/mtg-quality*', (r) => r.fulfill({ json: MTGQ }));
  await page.route('**/api/consulting/phone*', (r) => r.fulfill({ json: PHONE }));
  await page.route('**/api/crm/workspace/deals/*', (r) => r.fulfill({ json: GOALS }));
}

async function shot(page: Page, name: string) {
  const dir = process.env.CV2_SHOTS;
  if (!dir) return;
  mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: join(dir, `${name}.png`), animations: 'disabled' });
}

test.describe('コンサルKPI v2', () => {
  test.skip(pass === '', 'E2E_PASS が無いのでログインできない');
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await mockApi(page);
    await login(page);
  });

  test('ナビに「コンサルKPI」と「コンサルKPI v2」が並ぶ', async ({ page }) => {
    await page.goto('/');
    const nav = page.locator('a.tab-link');
    await expect(nav.filter({ hasText: /^コンサルKPI$/ })).toHaveAttribute('href', '/consulting');
    await expect(nav.filter({ hasText: /^コンサルKPI v2$/ })).toHaveAttribute('href', '/app/consulting-v2');
    await shot(page, '01-nav');
  });

  test('チームと担当 → 案件一覧 → 案件の詳細 → 案件管理へのリンク', async ({ page }) => {
    await page.goto('/app/consulting-v2');
    await expect(page.getByText('担当者ごとの状態（2 名）')).toBeVisible();
    await shot(page, '02-team');

    await page.getByRole('button', { name: '架空担当みどり' }).first().click();
    await expect(page.getByText(/3 件中 2 件/)).toBeVisible();
    await shot(page, '03-board');

    await page.getByRole('button', { name: '今日', exact: true }).click();
    await expect(page.getByText('今日動く先（2 件）')).toBeVisible();
    await shot(page, '04-today');

    await page.getByRole('button', { name: '案件一覧' }).click();
    await page.getByRole('button', { name: '架空株式会社A 本社' }).first().click();
    await expect(page.getByText('接触履歴（新しい順）')).toBeVisible();
    await expect(page.getByText('応募を月10件にする')).toBeVisible();
    await shot(page, '05-detail');
    await expect(page.getByRole('link', { name: /案件管理で確認/ }).first()).toHaveAttribute('href', '/app/job-copy?deal=70000000001');
  });

  test('顧客と拠点・満了と継続・成果と継続', async ({ page }) => {
    await page.goto('/app/consulting-v2?view=customer');
    await expect(page.getByText('法人を選ぶ（2 社）')).toBeVisible();
    await page.getByRole('button', { name: '架空株式会社A', exact: true }).click();
    await expect(page.getByText('拠点: 本社（契約 2 件）')).toBeVisible();
    await shot(page, '06-customer');
    await page.getByRole('button', { name: '満了と継続' }).click();
    await expect(page.getByText('今月 2026-11 に満了する契約（満了の近い順、1 件）')).toBeVisible();
    await shot(page, '07-renewal');
    await page.getByRole('button', { name: '成果と継続' }).click();
    await expect(page.getByText('月次の継続率（2026-09 に満了）')).toBeVisible();
    await expect(page.getByText('電話の到達')).toBeVisible();
    await shot(page, '08-results');
    await page.goto('/app/consulting-v2?view=board');
    await expect(page.getByRole('link', { name: 'HubSpot で開く' }).first()).toBeVisible();
    await shot(page, '09-board-columns');
    await page.goto('/app/consulting-v2');
    await expect(page.getByRole('img', { name: '担当ごとの1件あたりの接触回数の推移' })).toBeVisible();
    await shot(page, '10-team-trend');
  });

  test('第3段: 今日の絞り込みと成果の分布', async ({ page }) => {
    await page.goto('/app/consulting-v2?view=today');
    const card = page.getByRole('button', { name: /今週始まった契約/ });
    await card.click();
    await expect(card).toHaveAttribute('aria-pressed', 'true');
    const table = page.getByRole('table', { name: '今日動く案件' });
    await expect(table.getByRole('row')).toHaveCount(2);
    await expect(table).toContainText('架空株式会社B 第二拠点');
    await shot(page, '11-today-filter');
    await card.click();
    await expect(table.getByRole('row')).toHaveCount(3);
    await page.goto('/app/consulting-v2?view=results');
    const chart = page.getByRole('img', { name: '応募数の散らばり（継続回数ごと）' });
    await expect(chart.locator('rect')).toHaveAttribute('x', '320');
    await chart.scrollIntoViewIfNeeded();
    await shot(page, '12-results-box');
    await page.getByText('データの欠けと記入率の詳細').click();
    await expect(page.getByRole('table', { name: '結果ごとの記入率' })).toContainText('53.8%');
    await shot(page, '13-data-quality');
  });

  test('今のコンサルKPI (/consulting) は v2 の追加で変わらない', async ({ page }) => {
    const res = await page.goto('/consulting');
    expect(res?.status()).toBe(200);
    await expect(page.locator('#cs-side')).toBeVisible();
    await expect(page.locator('#cs-side button').first()).toHaveText('チームと担当');
  });
});
