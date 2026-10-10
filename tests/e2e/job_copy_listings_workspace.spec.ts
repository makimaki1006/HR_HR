import { test as sidebarSetup } from '@playwright/test';
sidebarSetup.beforeEach(async ({ page }) => { await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true')); });
import path from 'node:path';
import { test, expect } from '@playwright/test';
import { fixtureImage, fixtureHistory } from './job-copy-listings-fixture.mjs';
import { selectJobFeature, jobFeaturePanel, jobFeatures } from './job-copy-navigation';
const shots = path.resolve(__dirname, '../../docs/screenshots/job-copy-hubspot-ui');
test.beforeEach(async ({ page }) => {
  await page.route('https://example.invalid/job-copy/**', route => route.fulfill({ contentType: 'image/svg+xml', body: fixtureImage(route.request().url().includes('care')) }));
});
for (const width of [1440, 1920, 390]) {
  test(`${width}px: forty jobs share the workspace, and both media show body and images`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/app/job-copy?demo=1');
    await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
    const list = page.getByRole('region', { name: 'HubSpot の求人' });
    await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40);
    if (width === 390) {
      const cards = await list.getByRole('button', { name: /の版を見る$/ }).evaluateAll(elements => elements.filter(element => { const rect = element.getBoundingClientRect(); const parent = element.parentElement?.getBoundingClientRect(); return parent && rect.top >= Math.max(0, parent.top) && rect.bottom <= Math.min(window.innerHeight, parent.bottom); }));
      expect(cards.length).toBeGreaterThanOrEqual(5);
      await page.screenshot({ path: path.join(shots, 'list-390.png') });
    }
    const versions = page.waitForResponse(response => response.url().endsWith('/listings/1/versions'));
    await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
    const response = await (await versions).json();
    expect(response.versions).toHaveLength(2);
    expect(response.versions[1].body).toContain('基本給与 最小：280000');
    await expect(page.getByLabel('求人票')).toContainText('決まったルートで日用品を届けます。');
    await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
    const image = page.getByRole('region', { name: 'この版の掲載画像' }).locator('img');
    await expect(image).toBeVisible();
    await expect.poll(() => image.evaluate((element: HTMLImageElement) => element.naturalWidth)).toBe(800);
    const workspace = await page.locator('.jc-workspace').boundingBox();
    const sidebar = await page.locator('.jc-list').boundingBox();
    expect(workspace?.width).toBe(width);
    if (width > 800) {
      expect(sidebar!.height).toBeGreaterThan(750);
      expect(sidebar!.y + sidebar!.height).toBeCloseTo(1000, 0);
      expect(sidebar!.width).toBeGreaterThanOrEqual(320);
      expect((await page.locator('.jc-main').boundingBox())!.x).toBeGreaterThanOrEqual(sidebar!.width);
    } else {
      expect(sidebar).toBeNull();
      await expect(page.getByRole('button', { name: '一覧に戻る', exact: true })).toBeVisible();
      await expect(list).toBeHidden();
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
      expect(await page.evaluate(() => document.documentElement.scrollHeight - document.body.getBoundingClientRect().height)).toBeLessThan(2);
    }
    await page.evaluate(() => { window.scrollTo(0, 0); });
    await page.screenshot({ path: path.join(shots, `hrh-${width}.png`), fullPage: width === 390 });
    if (width > 800) {
      await page.getByLabel('求人票').getByRole('heading', { name: '給与', exact: true }).scrollIntoViewIfNeeded();
      await page.screenshot({ path: path.join(shots, `hrh-salary-${width}.png`) });
    }
    await selectJobFeature(page, 'timeline');
    await expect(jobFeaturePanel(page, 'timeline')).toContainText('月給28万');
    await selectJobFeature(page, 'diff');
    await expect(jobFeaturePanel(page, 'diff')).toContainText('月給 250,000円');
    if (width === 390) {
      await page.getByRole('button', { name: '一覧に戻る', exact: true }).click();
      await expect(list).toBeVisible();
      await expect(list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' })).toHaveAttribute('aria-pressed', 'true');
    }
    await list.getByRole('button', { name: '看護スタッフ・大分31の版を見る' }).click();
    await expect(page.getByLabel('求人票')).toContainText('落ち着いた環境で利用者の健康を支える看護のお仕事です。');
    const currentImage = page.getByRole('region', { name: '現在取得できる掲載画像' }).locator('img');
    await expect(currentImage).toBeVisible();
    await expect.poll(() => currentImage.evaluate((element: HTMLImageElement) => element.naturalWidth)).toBe(800);
    await expect(page.getByText('この版の保存時点の画像は不明です。', { exact: false })).toBeVisible();
    await page.evaluate(() => { window.scrollTo(0, 0); });
    await page.screenshot({ path: path.join(shots, `airwork-${width}.png`), fullPage: width === 390 });
    if (width === 390) {
      await page.getByRole('button', { name: '一覧に戻る', exact: true }).click();
      await expect(list.getByRole('button', { name: '看護スタッフ・大分31の版を見る' })).toBeInViewport();
    }
  });
}
test('preparing switches to forty jobs automatically; missing history and failed body have explicit messages', async ({ page, context }) => {
  await context.addCookies([{ name: 'jc_preparing', value: 'workspace', domain: '127.0.0.1', path: '/' }]);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  const list = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(list.getByText('求人の一覧を準備しています', { exact: true })).toBeVisible();
  await expect(list).toContainText('準備が終わると自動で一覧');
  await expect(list.locator('.jc-hubspot-list-heading')).toContainText('準備中');
  await expect(list.locator('.jc-hubspot-list-heading')).not.toContainText('未取得');
  await page.screenshot({ path: path.join(shots, 'preparing-1440.png') });
  await page.setViewportSize({ width: 1920, height: 1000 });
  await page.screenshot({ path: path.join(shots, 'preparing-1920.png') });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40, { timeout: 20_000 });
  await list.getByRole('button', { name: '看護スタッフ・大分37の版を見る' }).click();
  await expect(page.getByLabel('求人票')).toContainText('時給1,800円');
  await expect(page.getByRole('region', { name: '現在取得できる掲載画像' }).locator('img')).toBeVisible();
  await list.getByRole('button', { name: '看護スタッフ・東京39の版を見る' }).click();
  await expect(page.getByText('この求人の文面の履歴はありません。本文は未取得です。')).toBeVisible();
  await list.getByRole('button', { name: '看護スタッフ・大分40の版を見る' }).click();
  await expect(page.getByText(/この版の本文は未取得です/)).toBeVisible();
  await list.getByRole('button', { name: '看護スタッフ・沖縄38の版を見る' }).click();
  await expect(page.getByRole('heading', { name: '求人票を取得できませんでした' })).toBeVisible();
});

test('a pending version request shows a message in the detail before the body arrives', async ({ page }) => {
  let deliver: (() => Promise<void>) | undefined;
  await page.route('**/api/job-copy/listings/1/versions', route => { deliver = () => route.fulfill({ json: fixtureHistory('1') }); });
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  await page.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
  await expect(page.getByRole('heading', { name: '求人票を取得しています' })).toBeVisible();
  await expect(page.getByText('本文・画像・文面の履歴を確認しています。')).toBeVisible();
  await expect.poll(() => Boolean(deliver)).toBe(true);
  if (!deliver) throw new Error('synthetic request missing');
  await deliver();
  await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
});

test('390×844: mobile list shows at least four whole jobs, opens detail and returns without reloading', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  const list = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40);
  const viewport = await list.locator('.jc-list-scroll').boundingBox();
  expect(viewport!.height).toBeGreaterThan(360);
  const cards = await list.getByRole('button', { name: /の版を見る$/ }).evaluateAll(elements => elements.filter(element => { const rect = element.getBoundingClientRect(); const parent = element.parentElement?.getBoundingClientRect(); return parent && rect.top >= Math.max(0, parent.top) && rect.bottom <= Math.min(window.innerHeight, parent.bottom); }));
  expect(cards.length).toBeGreaterThanOrEqual(4);
  await page.screenshot({ path: path.join(shots, 'list-390-844.png') });
  await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
  await expect(page.getByLabel('求人票')).toContainText('月給 280,000円〜320,000円');
  await page.getByRole('button', { name: '一覧に戻る', exact: true }).click();
  await expect(list).toBeVisible();
});

test('390px: incomplete history warning stays visible in detail and raw IDs are absent', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 1000 });
  await page.route('**/api/job-copy/listings/1/versions', route => route.fulfill({ json: { ...fixtureHistory('1'), versions: Array.from({ length: 20 }, (_, index) => ({ ...fixtureHistory('1').versions[index % 2], written_at: `2026-09-${String(index + 1).padStart(2, '0')}T00:00:00Z` })), history_may_be_incomplete: true, history_counts: { hrh_kyuujinhyou_honbun: 20, hrh_kyuujinhyou_gazou: 2 } } }));
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  const list = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40);
  await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
  await expect(page.locator('.jc-main').getByText(/過去の版がすべて含まれているとは限りません/)).toBeVisible();
  await expect(page.locator('.jc-main').getByText(/過去の版がすべて含まれているとは限りません/)).toBeInViewport();
  await expect(page.locator('.jc-main')).not.toContainText(/HR-|HubSpot求人ID/);
  await page.screenshot({ path: path.join(shots, 'history-warning-390.png') });
});

test('1440px: listing and detail do not expose raw IDs', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto('/app/job-copy?demo=1');
  await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
  const list = page.getByRole('region', { name: 'HubSpot の求人' });
  await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40);
  await expect(list).not.toContainText(/HR-|HubSpot求人ID/);
  await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
  await expect(page.getByLabel('求人票')).toContainText('決まったルート');
  await expect(page.locator('.jc-main')).not.toContainText(/HR-|HubSpot求人ID/);
});

for (const width of [1440, 1920]) {
  test(`${width}px: all job-copy features identify jobs by media and title without raw IDs`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    await page.goto('/app/job-copy?demo=1');
    await page.getByRole('button', { name: 'HubSpot の求人', exact: true }).click();
    const list = page.getByRole('region', { name: 'HubSpot の求人' });
    await list.getByRole('button', { name: '配送ドライバー・沖縄2の版を見る' }).click();
    await expect(page.getByLabel('求人票')).toContainText('決まったルート');
    await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
    await expect(page.getByLabel('求人票')).toContainText('決まったルート');
    for (const feature of Object.keys(jobFeatures) as (keyof typeof jobFeatures)[]) {
      await test.step(jobFeatures[feature].label, async () => {
        await selectJobFeature(page, feature);
        await expect(jobFeaturePanel(page, feature)).toBeVisible();
        if (feature === 'ab') await page.getByRole('combobox', { name: 'Bとして比較する求人', exact: true }).selectOption({ label: '取引先名未取得 · 配送ドライバー・沖縄2 · HRハッカー' });
        expect(await page.locator('.jc-app').innerText()).not.toMatch(/HR-|AW-|HubSpot求人ID|求人ID|店舗ID|sample-account/);
        if (['performance', 'report', 'ab'].includes(feature) && width === 1440) {
          await page.evaluate(() => { window.scrollTo(0, 0); });
          await page.screenshot({ path: path.join(shots, `${feature}-1440.png`) });
        }
      });
    }
  });
}
