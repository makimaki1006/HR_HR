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
  await expect(primary.getByRole('tab').first()).toHaveText('タイムライン');
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
  await expect(timeline.getByRole('group', { name: '課金', exact: true })).toContainText('4.5万円');
  await expect(timeline.getByText('2026年09月以降は市場データがありません（2026年08月まで）')).toBeVisible();
  const rows = timeline.getByRole('table').locator('tbody tr');
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0).locator('td')).toHaveText(['14日', '7件', '0.50件/日', '3万円', 'データなし']);
  await expect(rows.nth(1).locator('td')).toHaveText(['10日', '8件', '0.80件/日', '約2.8万円', 'データなし']);
  await page.screenshot({ path: `${shots}/timeline-1280.png`, fullPage: true });

  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  const overview = page.getByRole('region', { name: '求人の横断比較の表' });
  await expect(overview.locator('tbody tr').first()).toContainText('0.64件/日');
  await expect(overview.locator('tbody tr').first()).toContainText('8.7万円');
  await page.screenshot({ path: `${shots}/overview-1280.png`, fullPage: true });

  await page.setViewportSize({ width: 375, height: 800 });
  await page.getByRole('button', { name: '一覧', exact: true }).click();
  await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await page.screenshot({ path: `${shots}/timeline-375.png`, fullPage: true });
  expect(requests).toEqual([]);
});
