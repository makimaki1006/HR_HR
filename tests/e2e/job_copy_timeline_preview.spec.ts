/**
 * Job timeline on the local demo preview (fictional data, no backend). Run:
 *   (cd frontend && npx vite --port 5297 --strictPort --host 127.0.0.1)
 *   npx playwright test -c tests/e2e/job-copy-timeline-preview.config.ts
 * Checks rendered values and that both ECharts lanes finished rendering (not just a canvas).
 */
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { test, expect } from '@playwright/test';

const shots = resolve('data/job-copy-local/timeline-preview');
test.beforeAll(() => { mkdirSync(shots, { recursive: true }); });

test('timeline lanes, chart readiness and period values on the demo job', async ({ page }) => {
  const requests: string[] = [];
  page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/')) requests.push(request.url()); });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/static/app/job-copy-preview.html');
  const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
  await expect(primary.getByRole('tab')).toHaveText(['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']);
  await expect(primary.getByRole('tab', { name: 'タイムライン', exact: true })).toHaveAttribute('aria-selected', 'true');
  const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
  for (const lane of ['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場']) await expect(timeline.getByRole('group', { name: lane, exact: true })).toBeVisible();
  for (const id of ['jt-applications', 'jt-market']) {
    const chart = page.locator(`[data-testid="${id}"][data-chart-ready="true"]`);
    await expect(chart).toHaveCount(1);
    const points = await chart.evaluate(element => {
      const instance = window.__echarts_getInstanceByDom?.(element as HTMLElement);
      const option = instance?.getOption() as { series?: { data?: unknown[] }[] } | undefined;
      return option?.series?.map(series => series.data?.length ?? 0) ?? [];
    });
    expect(points.length).toBeGreaterThan(0);
    expect(points[0]).toBeGreaterThan(0);
  }
  await expect(timeline.getByRole('group', { name: '給与', exact: true })).toContainText('月給25万〜28万円');
  await expect(timeline.getByRole('group', { name: '給与', exact: true })).toContainText('月給27万〜30万円');
  // 上がった版は ▲、下がった版は ▼（下がったのに ▲ を付けない）
  await expect(timeline.getByRole('group', { name: '給与', exact: true }).locator('.jt-salary-label')).toHaveText(['月給25万〜28万円', '▲月給27万〜30万円', '▼月給25万〜28万円']);
  // 一覧の件数はタイムラインと同じ（架空の件数と分かるように書く）
  await expect(page.locator('.jc-job').first()).toContainText('応募28件（架空）');
  await expect(page.locator('.jc-job').first()).not.toContainText('応募未取得');
  await expect(timeline.getByRole('group', { name: '課金', exact: true })).toContainText('4万5,000円');
  // 市場データの最後の月はデータから読む（毎月更新）。それより後の期間は、どの月までデータがあるかを書く。
  await expect(timeline.getByText('市場データは2026年8月まで（毎月更新）。2026年9月以降はデータなしとして表示しています', { exact: true })).toBeVisible();
  const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).getByRole('table').locator('tbody tr');
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0).locator('td')).toHaveText(['14日', '7件', '0.50件/日', 'デモ用の架空の金額 3万円', 'データなし（市場求人数は2026年8月まで）']);
  await expect(rows.nth(1).locator('td')).toHaveText(['10日', '8件', '0.80件/日', 'デモ用の架空の金額 約2万8,125円', 'データなし（市場求人数は2026年8月まで）']);
  await page.screenshot({ path: `${shots}/timeline-1280.png`, fullPage: true });

  // First view (2026-10-08 layout): one-line top bar, full-height list, timeline lanes on screen.
  await page.setViewportSize({ width: 1100, height: 623 });
  const firstView = await page.evaluate(() => {
    const box = (selector: string) => document.querySelector(selector)?.getBoundingClientRect();
    const lane = [...document.querySelectorAll('.jt-timeline [role="group"]')][0]?.getBoundingClientRect();
    return { inner: window.innerHeight, top: box('.jc-topline')?.height ?? 0, list: box('.jc-list')?.bottom ?? 0, scroll: box('.jc-list-scroll')?.height ?? 0, lane: lane?.top ?? 9999 };
  });
  expect(firstView.top).toBeLessThan(60);
  expect(firstView.list).toBeGreaterThanOrEqual(firstView.inner - 2);
  expect(firstView.scroll).toBeGreaterThan(250);
  expect(firstView.lane).toBeLessThan(firstView.inner);
  await expect(page.locator('.jc-data-import')).toBeHidden();
  await expect(page.locator('.jc-reverse-search')).toHaveCount(0);
  await page.screenshot({ path: `${shots}/first-view-1100x623.png` });
  await page.setViewportSize({ width: 1280, height: 900 });

  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  const overview = page.getByRole('region', { name: '求人の横断比較の表' });
  await expect(overview.locator('tbody tr').first()).toContainText('0.80件/日');
  await expect(overview.locator('tbody tr').first()).toContainText('8万7,000円');
  // 実際の課金データが無い求人は、仮の課金データ（ダミー）を合計しない（タイムラインの課金の段にだけ出す）
  await expect(overview.locator('tbody tr', { hasText: '倉庫内ピッキングスタッフ' }).locator('td.jo-billing')).toHaveText('実際の課金データなし（仮の課金データ（ダミー）は合計しません）');
  await expect(overview.locator('tbody tr', { hasText: '地域配送ドライバー' }).locator('td.jo-billing')).toHaveText('デモ用の架空の金額 8万7,000円');
  await expect(page.getByText('仮の課金データ（ダミー）は架空の金額です。この表の課金合計にも並び替えにも使っていません。金額は各求人のタイムラインの「課金」の段で「ダミー」と付けて表示します。', { exact: true })).toBeVisible();
  // 並び替えの理由ではないことを、並び替えの横に文字で示す
  await expect(page.getByText('並び順は数の大小で並べただけです。応募が増えた・減った理由を示すものではありません。', { exact: true })).toBeVisible();
  await expect(overview).not.toContainText('変更日');
  await expect(overview).not.toContainText('未接続');
  await page.screenshot({ path: `${shots}/overview-1280.png`, fullPage: true });
  // 1100 幅でも 7 列（課金合計まで）が横スクロールなしで収まる
  await page.setViewportSize({ width: 1100, height: 623 });
  const fit = await overview.evaluate(element => ({ scroll: element.scrollWidth, client: element.clientWidth, billing: element.querySelector('thead th:last-child')?.getBoundingClientRect().right ?? 9999, right: element.getBoundingClientRect().right }));
  expect(fit.scroll, `横断比較 ${String(fit.scroll)} > ${String(fit.client)}`).toBeLessThanOrEqual(fit.client);
  expect(fit.billing).toBeLessThanOrEqual(fit.right + 0.5);
  await page.screenshot({ path: `${shots}/overview-1100.png` });
  await page.setViewportSize({ width: 1280, height: 900 });

  await page.setViewportSize({ width: 375, height: 800 });
  await page.getByRole('button', { name: '一覧', exact: true }).click();
  await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await page.screenshot({ path: `${shots}/timeline-375.png`, fullPage: true });
  expect(requests).toEqual([]);
});

test('billing CSV import matches on media + account + job ID and fills the billing lane and the overview total', async ({ page }) => {
  const requests: string[] = [];
  page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/')) requests.push(request.url()); });
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.goto('/static/app/job-copy-preview.html');
  const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
  const billingLane = timeline.getByRole('group', { name: '課金', exact: true });
  // The demo's HRハッカー amounts are made up and say so (the demo banner is not printed).
  await expect(billingLane.locator('.jt-billing')).toHaveText(['架空 3万円', '架空 4万5,000円', '架空 1万2,000円']);
  await page.getByRole('button', { name: 'データ取込', exact: true }).click();
  await expect(page.getByRole('region', { name: 'データ取込', exact: true })).toBeVisible();
  // 媒体 + 店舗ID（Airワークは口座ログインID）+ 媒体求人ID で結びつける。HRハッカーの媒体求人IDは 8 桁の数字でないと使わない。
  const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,DEMO-SHOP-01,DEMO-HRH-001,2026-09-01,2026-09-14,33000\nAirワーク,DEMO-ACCOUNT-01,DEMO-AIR-002,2026-09-05,2026-09-30,40000\n';
  await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
  await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
  await expect(page.getByText('値に誤りがあり使わない行（1行）')).toBeVisible();
  await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
  await expect(page.getByText(/課金CSVの 1 期間を反映中/u)).toBeVisible();
  await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
  await expect(page.locator('.jc-data-import')).toBeHidden();
  // The HRハッカー row was not used, so demo-001 keeps its HRハッカー実績.
  await expect(billingLane.locator('.jt-billing')).toHaveText(['架空 3万円', '架空 4万5,000円', '架空 1万2,000円']);
  await expect(billingLane.locator('.jt-billing-csv')).toHaveCount(0);
  const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).getByRole('table').locator('tbody tr');
  await expect(rows.nth(0).locator('td')).toHaveText(['14日', '7件', '0.50件/日', 'デモ用の架空の金額 3万円', 'データなし（市場求人数は2026年8月まで）']);
  await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
  await page.locator('.jc-job', { hasText: '倉庫内ピッキングスタッフ' }).click();
  await expect(timeline.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。', { exact: true })).toBeVisible();
  // CSV の 09-05〜09-30 は CSV の金額。CSV に無い 10-01〜10-05 だけ仮の課金データ（ダミー）が残る。
  await expect(billingLane.locator('.jt-billing')).toHaveText(['4万円', 'ダミー 6,935円']);
  await expect(billingLane.locator('.jt-billing-dummy')).toHaveCount(1);
  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  const overview = page.getByRole('region', { name: '求人の横断比較の表' });
  await expect(overview.locator('tbody tr', { hasText: '倉庫内ピッキングスタッフ' }).locator('td.jo-billing')).toHaveText('4万円（仮の課金データ（ダミー）は合計に入れていません）');
  await expect(overview.locator('tbody tr', { hasText: '地域配送ドライバー' })).toContainText('8万7,000円');
  await page.screenshot({ path: `${shots}/overview-billing-1280.png`, fullPage: true });
  // Reloading drops the browser-only billing rows (the URL keeps the selected demo-job-002, which has no HRハッカー実績).
  await page.reload();
  const reloadedLane = page.getByRole('region', { name: 'タイムライン', exact: true }).getByRole('group', { name: '課金', exact: true });
  await expect(page.locator('.jc-detail h1')).toHaveText('倉庫内ピッキングスタッフ');
  await expect(reloadedLane.locator('.jt-billing-csv')).toHaveCount(0);
  await expect(reloadedLane.locator('.jt-billing')).toHaveText(['ダミー 6万5,867円', 'ダミー 6,935円']);
  await expect(reloadedLane).toContainText('仮の課金データ（ダミー）');
  expect(requests).toEqual([]);
});
