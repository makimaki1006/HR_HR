import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';
import { openOtherAnalysis } from '../job-copy-navigation';

test('変更の印から前後の応募とグラフの帯を表示', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route('**/api/job-copy/**', route => route.fulfill({ json: {} }));
  await login(page);
  await page.goto('/app/job-copy?demo=1&job=demo-job-001');
  await openOtherAnalysis(page, 'タイムライン');
  const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
  const marks = timeline.getByRole('group', { name: '本文', exact: true }).getByRole('button');
  await marks.nth(1).click();
  const comparison = timeline.getByRole('region', { name: '変えた前後の応募' });
  await expect(comparison).toContainText('前 14日 7件');
  await expect(comparison).toContainText('後 10日分（14日に届かず） 8件');
  await expect(comparison).toContainText('1日あたり 0.5件');
  await expect(comparison).toContainText('1日あたり 0.8件');
  const chart = timeline.getByTestId('jt-applications');
  const bands = () => chart.evaluate(el => {
    const instance = window.__echarts_getInstanceByDom?.(el as HTMLElement);
    if (!instance) return null;
    const option = instance.getOption() as { series: { data: unknown[]; markArea: { data: { xAxis: number }[][] } }[] };
    return { count: option.series[0].data.length, bands: option.series[0].markArea.data.map(pair => pair.map(point => point.xAxis)) };
  });
  await expect.poll(bands).toEqual({ count: 5, bands: [
    [Date.parse('2026-09-01T00:00:00Z'), Date.parse('2026-09-15T00:00:00Z')],
    [Date.parse('2026-09-15T00:00:00Z'), Date.parse('2026-09-25T00:00:00Z')],
  ] });
  for (const unit of ['日ごと', '月ごと', '週ごと']) {
    await timeline.getByRole('button', { name: unit, exact: true }).click();
    await expect.poll(async () => (await bands())?.bands.length).toBe(2);
  }
  await comparison.getByRole('button', { name: '7日', exact: true }).click();
  await expect(comparison).toContainText('前 7日 3件');
  await expect(comparison).toContainText('後 7日 7件');
  await comparison.getByRole('button', { name: '28日', exact: true }).click();
  await expect(comparison).toContainText('前 28日 7件');
  await comparison.getByRole('button', { name: '14日', exact: true }).click();
  await expect(chart).toHaveAttribute('data-chart-ready', 'true');
  await comparison.scrollIntoViewIfNeeded();
  expect(await comparison.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  const sides = comparison.locator('.jt-window-side');
  const left = await sides.nth(0).boundingBox(); const right = await sides.nth(1).boundingBox();
  expect(left!.x + left!.width).toBeLessThanOrEqual(right!.x);
  const folder = process.env.CHANGE_WINDOW_SHOTS_DIR;
  if (folder) {
    await mkdir(folder, { recursive: true });
    await page.screenshot({ path: `${folder}/change-window-1440.png` });
  }
});
