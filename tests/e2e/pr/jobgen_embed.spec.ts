import { test, expect } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { login } from './helpers/login';
const shots = path.resolve(__dirname, '../../../docs/screenshots/job-copy-ui-polish');
for (const width of [1440, 1920]) {
  test(`${width}px: 求人票作成の枠が中身に追従し、切替後も入力を保持する`, async ({ page }) => {
    await login(page);
    await page.setViewportSize({ width, height: 900 });
    await page.route('**/api/jobgen/**', route => route.fulfill({ status: 503, json: { error: '合成テストでは生成しません' } }));
    if (width === 1440 && !process.env.CI) {
      const old = execFileSync('git', ['show', '57a25088:src/job_gen/handlers.rs'], { encoding: 'utf8' });
      const html = old.slice(old.indexOf('r#"<div class="space-y-4">') + 3, old.indexOf('</div>"#,') + 6);
      await page.route('**/tab/jobgen_tools', route => route.fulfill({ contentType: 'text/html', body: html }));
      await page.goto('/tab/jobgen_tools');
      await expect.poll(() => page.locator('#jt-frame-0').evaluate((frame: HTMLIFrameElement) => frame.contentDocument?.readyState)).toBe('complete');
      expect(await page.locator('#jt-frame-0').evaluate((frame: HTMLIFrameElement) => frame.contentDocument!.documentElement.scrollHeight - frame.clientHeight)).toBeGreaterThan(100);
      await page.screenshot({ path: path.join(shots, 'before-jobgen-1440.png') });
      await page.unroute('**/tab/jobgen_tools');
    }
    await page.goto('/tab/jobgen_tools');
    const frame = page.locator('#jt-frame-0');
    await expect.poll(() => frame.evaluate((frame: HTMLIFrameElement) => frame.contentDocument?.readyState)).toBe('complete');
    await expect.poll(() => frame.evaluate((frame: HTMLIFrameElement) => frame.contentDocument!.documentElement.scrollHeight - frame.clientHeight)).toBeLessThanOrEqual(1);
    const input = page.frameLocator('#jt-frame-0').locator('textarea').first();
    await input.fill('合成の求人票。月給250,000円。');
    const oldHeight = await frame.evaluate(frame => frame.clientHeight);
    await frame.evaluate((frame: HTMLIFrameElement) => { const extra = frame.contentDocument!.createElement('div'); extra.id = 'synthetic-extra'; extra.style.height = '700px'; frame.contentDocument!.body.append(extra); });
    await expect.poll(() => frame.evaluate(frame => frame.clientHeight)).toBeGreaterThanOrEqual(oldHeight + 699);
    await frame.evaluate((frame: HTMLIFrameElement) => frame.contentDocument!.getElementById('synthetic-extra')!.remove());
    await expect.poll(() => frame.evaluate(frame => frame.clientHeight)).toBe(oldHeight);
    await page.getByRole('tab', { name: '競合比較から求人作成' }).click();
    await page.getByRole('tab', { name: '求人票生成', exact: true }).click();
    await expect(input).toHaveValue('合成の求人票。月給250,000円。');
    await expect.poll(() => frame.evaluate((frame: HTMLIFrameElement) => frame.contentDocument!.documentElement.scrollHeight - frame.clientHeight)).toBeLessThanOrEqual(1);
    if (width === 1440) await page.screenshot({ path: path.join(shots, 'after-jobgen-1440.png') });
  });
}
