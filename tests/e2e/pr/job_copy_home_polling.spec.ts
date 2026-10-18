import { expect, test } from '@playwright/test';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { demoOwnerTodo } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { login } from './helpers/login';

test('集計が続くホームを60秒開いたときの呼び出し回数', async ({ page }) => {
  test.setTimeout(100_000);
  let home = 0; let recent = 0;
  await page.route('**/api/job-copy/home*', route => {
    home++;
    return route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, todos_status: 'preparing', refreshing: true } } });
  });
  await page.route('**/api/job-copy/recent-changes*', route => {
    recent++;
    return route.fulfill({ json: { status: 'ready', report: { count: 0, listings: [] }, refresh_failed: false } });
  });
  await login(page);
  await page.goto('/app/job-copy?owner_field=consultant&owner=11');
  await expect(page.getByText('やることを集計しています…', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: '顧客', exact: true })).toBeVisible();
  await page.waitForTimeout(60_000);
  console.log(JSON.stringify({ seconds: 60, home, recent }));
  // The sixth read is scheduled at 60s; response time can put it just outside the window.
  expect(home).toBeGreaterThanOrEqual(5); expect(home).toBeLessThanOrEqual(6);
  expect(recent).toBe(1);
});
