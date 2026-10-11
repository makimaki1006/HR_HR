import { test, expect } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';
const counts = (count: number, first_at: string | null = null, last_at: string | null = null) => ({ count, first_at, last_at });
const summary = {
  opens: counts(2, '2026-10-12T04:00:00Z', '2026-10-12T05:03:00Z'), clicks: counts(1),
  automatic_opens: counts(3), automatic_clicks: counts(1),
  links: [{ link_no: 1, url: 'https://example.invalid/guide', clicks: counts(1), automatic_clicks: counts(1) }],
};
for (const tracked of [true, false]) {
  test(tracked ? '計測ありのメールカード' : '計測なしのメールカード', async ({ page }) => {
    const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
    await page.route('**/api/crm/emails/*/tracking', route => route.fulfill(tracked ? { status: 200, contentType: 'application/json', body: JSON.stringify(summary) } : { status: 204 }));
    await page.goto(`/static/app/__tracking_preview?tracked=${tracked ? '1' : '0'}`);
    await expect(page.getByTestId('activity-card')).toBeVisible();
    if (tracked) {
      await expect(page.getByTestId('email-tracking')).toContainText('開封 2 回（最後 10/12 14:03）・クリック 1 回');
      await expect(page.getByLabel('開封・クリックの詳細', { exact: true })).toHaveText('開封 2 回（最後 10/12 14:03）・クリック 1 回');
      await expect(page.getByTestId('email-tracking').locator('details')).not.toHaveAttribute('open');
      await expect(page.getByTestId('email-tracking-automatic')).toBeHidden();
      await page.getByLabel('開封・クリックの詳細', { exact: true }).click();
      await expect(page.getByTestId('email-tracking-automatic')).toHaveText('ほかに自動の確認とみられるもの：開封 3 回・クリック 1 回（数に含めていません）');
      await expect(page.getByText('https://example.invalid/guide', { exact: true }).locator('..')).toHaveText('https://example.invalid/guide：クリック 1 回（ほかに自動 1 回）');
      await expect(page.getByTestId('email-tracking')).toContainText('最初の開封 10/12 13:00');
      await page.getByLabel('開封・クリックの詳細', { exact: true }).click();
      await expect(page.getByTestId('email-tracking-automatic')).toBeHidden();
      await expect(page.getByTestId('email-tracking').locator('details')).not.toHaveAttribute('open');
    } else {
      await expect(page.getByTestId('activity-title')).toHaveText('先日のご相談について（架空）');
      await expect(page.getByTestId('email-tracking')).toHaveCount(0);
    }
    expect(errors).toEqual([]);
    const out = process.env.SCREENSHOT_DIR ?? 'test-results/email-tracking'; await mkdir(out, { recursive: true });
    await page.screenshot({ path: path.join(out, tracked ? 'email-tracked.png' : 'email-untracked.png'), fullPage: true });
  });
}
