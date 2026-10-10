import path from 'node:path';
import { test, expect } from '@playwright/test';
import { fixtureJobs, fixtureHistory } from './job-copy-listings-fixture.mjs';

const triggerName = '求人一覧を開く';
export function sidebarTests() {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1000 });
    await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: fixtureJobs.length, index_built_at: '2026-10-10T00:00:00Z', listings: fixtureJobs, titles: ['ドライバー', '看護師'], offset: 0, next_offset: null, refreshing: false, refresh_failed: false } }));
    await page.route('**/api/job-copy/listings/*/versions', route => route.fulfill({ json: fixtureHistory('1') }));
    await page.route('https://example.invalid/**', route => route.fulfill({ status: 404, body: '' }));
    await page.goto('/app/job-copy?demo=1');
  });

  for (const width of [1440, 1920]) {
    test(`${width}px: マウスで開き、求人選択で閉じ、詳細が残り幅を使う`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      const trigger = page.getByRole('button', { name: triggerName });
      await expect(trigger).toHaveAttribute('aria-expanded', 'false');
      const workspace = (await page.locator('.jc-workspace').boundingBox())!;
      const detail = (await page.locator('.jc-main').boundingBox())!;
      expect(detail.x - workspace.x).toBe(48);
      expect(detail.width).toBe(width - 48);
      const shots = path.resolve(__dirname, '../../docs/screenshots/job-copy-collapsible-sidebar');
      if (width === 1440 && process.env.JOB_COPY_SIDEBAR_SCREENSHOTS) await page.screenshot({ path: path.join(shots, 'closed-1440.png') });
      await trigger.hover();
      await expect(trigger).toHaveAttribute('aria-expanded', 'true');
      await expect(page.getByRole('complementary', { name: '求人一覧と絞り込み' })).toBeVisible();
      expect((await page.locator('.jc-main').boundingBox())!.width).toBe(width - 48);
      if (width === 1440 && process.env.JOB_COPY_SIDEBAR_SCREENSHOTS) {
        await expect.poll(() => page.locator('.jc-list').evaluate(element => getComputedStyle(element).opacity)).toBe('1');
        await page.screenshot({ path: path.join(shots, 'open-1440.png') });
      }
      const card = page.locator('.jc-fixed-list .jc-job').nth(1);
      const title = await card.locator('strong').innerText();
      await card.click();
      await expect(trigger).toHaveAttribute('aria-expanded', 'false');
      await expect(page.locator('.jc-detail-heading h1')).toHaveText(title);
      await expect(page.locator('#job-details')).toBeFocused();
      await trigger.hover();
      await page.getByRole('button', { name: '固定', exact: true }).click();
      expect((await page.locator('.jc-main').boundingBox())!.width).toBe(width - 300);
      if (width === 1440 && process.env.JOB_COPY_SIDEBAR_SCREENSHOTS) await page.screenshot({ path: path.join(shots, 'pinned-1440.png') });
    });
  }

  for (const width of [1440, 1920]) {
    test(`${width}px: HubSpot一覧をEsc・求人選択・固定解除で閉じても帯は48px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      const trigger = page.getByRole('button', { name: triggerName });
      const panel = page.getByRole('complementary', { name: '求人一覧と絞り込み' });
      const region = page.getByRole('region', { name: 'HubSpot の求人' });
      async function openList() {
        await page.mouse.move(width - 100, 500);
        await trigger.hover();
        await expect(trigger).toHaveAttribute('aria-expanded', 'true');
        await expect(panel).toBeVisible();
      }
      async function expectFullDetail() {
        const workspace = (await page.locator('.jc-workspace').boundingBox())!;
        const detail = (await page.locator('.jc-main').boundingBox())!;
        expect((await page.locator('.jc-sidebar-rail').boundingBox())!.width).toBe(48);
        expect(detail.x - workspace.x).toBe(48);
        expect(detail.width).toBe(width - 48);
      }
      const shots = path.resolve(__dirname, '../../docs/screenshots/job-copy-collapsible-sidebar');
      async function screenshot(state: string) {
        if (width !== 1440 || !process.env.JOB_COPY_SIDEBAR_SCREENSHOTS) return;
        await expect.poll(() => page.locator('.jc-list').evaluate(element => getComputedStyle(element).opacity)).toBe(state === 'closed' ? '0' : '1');
        await page.screenshot({ path: path.join(shots, `hubspot-${state}-1440.png`) });
      }
      await openList();
      await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
      await expect(region.locator('.jc-job')).toHaveCount(40);
      await expectFullDetail();
      await page.keyboard.press('Escape');
      await expect(trigger).toHaveAttribute('aria-expanded', 'false');
      await expect(trigger).toBeFocused();
      await expectFullDetail();
      await openList();
      let deliver: (() => Promise<void>) | undefined;
      await page.route('**/api/job-copy/listings/1/versions', route => { deliver = () => route.fulfill({ json: fixtureHistory('1') }); });
      await region.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
      // 文面が届く前から一覧を閉じ、詳細へフォーカスを移す。
      await expect(page.getByRole('heading', { name: '求人票を取得しています' })).toBeVisible();
      await expect(trigger).toHaveAttribute('aria-expanded', 'false');
      await expect(page.locator('#job-details')).toBeFocused();
      await expect.poll(() => Boolean(deliver)).toBe(true);
      if (!deliver) throw new Error('合成求人の取得待ちがありません');
      await deliver();
      await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
      await expectFullDetail();
      await screenshot('closed');
      await openList();
      await expectFullDetail();
      await screenshot('open');
      await page.getByRole('button', { name: '固定', exact: true }).click();
      const pinnedWidth = Math.min(420, Math.max(320, width * 0.25));
      expect((await page.locator('.jc-sidebar').boundingBox())!.width).toBe(pinnedWidth);
      expect((await page.locator('.jc-main').boundingBox())!.width).toBe(width - pinnedWidth);
      await screenshot('pinned');
      await page.getByRole('button', { name: '固定を解除', exact: true }).click();
      await expect(trigger).toHaveAttribute('aria-expanded', 'false');
      await expect(trigger).toBeFocused();
      await expectFullDetail();
    });
  }

  test('Tab・Esc・外クリックと、マウスが少し離れたときの猶予', async ({ page }) => {
    const trigger = page.getByRole('button', { name: triggerName });
    await trigger.focus();
    await expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect(page.getByRole('complementary', { name: '求人一覧と絞り込み' })).toBeVisible();
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: '固定', exact: true })).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: 'HubSpot の求人', exact: true })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await expect(trigger).toBeFocused();
    await page.keyboard.press('Tab');
    expect(await page.locator('.jc-list').evaluate(element => element.contains(document.activeElement))).toBe(false);
    await trigger.hover();
    await page.mouse.move(1100, 500);
    await page.waitForTimeout(100);
    await expect(trigger).toHaveAttribute('aria-expanded', 'true');
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await trigger.hover();
    await page.mouse.click(1100, 220);
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await page.keyboard.press('Tab');
    await trigger.focus();
    await page.keyboard.press('Tab');
    await page.mouse.click(1100, 220);
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(await page.locator('.jc-list').evaluate(element => element.contains(document.activeElement))).toBe(false);
  });

  test('固定を記憶し、解除で閉じた状態を記憶する', async ({ page }) => {
    const trigger = page.getByRole('button', { name: triggerName });
    await trigger.hover();
    await page.getByRole('button', { name: '固定', exact: true }).click();
    await page.locator('.jc-fixed-list .jc-job').nth(1).click();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('button', { name: '固定を解除', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.reload();
    await expect(page.getByRole('button', { name: '固定を解除', exact: true })).toBeVisible();
    await page.getByRole('button', { name: '固定を解除', exact: true }).click();
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await expect(trigger).toBeFocused();
    await page.reload();
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  test('保存先が読めなくても開閉でき、動きを減らす設定に従う', async ({ page }) => {
    await page.addInitScript(() => {
      Storage.prototype.getItem = () => { throw new Error('blocked'); };
      Storage.prototype.setItem = () => { throw new Error('blocked'); };
    });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.reload();
    const trigger = page.getByRole('button', { name: triggerName });
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await trigger.hover();
    expect(await page.locator('.jc-list').evaluate(element => getComputedStyle(element).transitionDuration)).toBe('0s');
    await page.getByRole('button', { name: '固定', exact: true }).click();
    await page.getByRole('button', { name: '固定を解除', exact: true }).click();
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
  });

  test('固定一覧の検索条件・選択・スクロールを開閉で失わない', async ({ page }) => {
    const trigger = page.getByRole('button', { name: triggerName });
    await trigger.hover();
    await page.getByLabel('求人名・企業名・勤務地で検索').fill('デモ');
    await page.locator('.jc-fixed-list .jc-filter-more > summary').click();
    await page.locator('.jc-fixed-list').getByLabel('並び順', { exact: true }).selectOption('applications');
    const scroll = page.locator('.jc-fixed-list .jc-list-scroll');
    await scroll.evaluate(element => { element.scrollTop = 180; });
    const before = await scroll.evaluate(element => element.scrollTop);
    expect(before).toBeGreaterThan(0);
    const selected = await page.locator('.jc-fixed-list .jc-job[aria-pressed="true"] strong').innerText();
    await page.keyboard.press('Escape');
    await page.mouse.move(1100, 500);
    await trigger.hover();
    await expect(page.getByLabel('求人名・企業名・勤務地で検索')).toHaveValue('デモ');
    await expect(page.locator('.jc-fixed-list').getByLabel('並び順', { exact: true })).toHaveValue('applications');
    expect(await scroll.evaluate(element => element.scrollTop)).toBe(before);
    await expect(page.locator('.jc-fixed-list .jc-job[aria-pressed="true"] strong')).toHaveText(selected);
  });

  test('HubSpot の求人を選ぶと閉じ、絞り込みとスクロールを保持する', async ({ page }) => {
    const trigger = page.getByRole('button', { name: triggerName });
    await trigger.hover();
    await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
    const region = page.getByRole('region', { name: 'HubSpot の求人' });
    await expect(region.locator('.jc-job')).toHaveCount(40);
    const filter = region.getByLabel('媒体', { exact: true });
    await filter.selectOption({ index: 1 });
    const value = await filter.inputValue();
    const scroll = region.locator('.jc-list-scroll');
    await scroll.evaluate(element => { element.scrollTop = 150; });
    const before = await scroll.evaluate(element => element.scrollTop);
    expect(before).toBeGreaterThan(0);
    await page.keyboard.press('Escape');
    await page.mouse.move(1100, 500);
    await trigger.hover();
    await expect(filter).toHaveValue(value);
    expect(await scroll.evaluate(element => element.scrollTop)).toBe(before);
    await region.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
  });
}
