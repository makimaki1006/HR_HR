import path from 'node:path';
import { test, expect } from '@playwright/test';
import { fixtureImage, fixtureHistory } from './job-copy-listings-fixture.mjs';
import { selectJobFeature, jobFeaturePanel } from './job-copy-navigation';
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
    const versions = page.waitForResponse(response => response.url().endsWith('/listings/1/versions'));
    await list.getByRole('button', { name: '配送ドライバー・大分1の版を見る' }).click();
    const response = await (await versions).json();
    expect(response.versions).toHaveLength(2);
    expect(response.versions[1].body).toContain('給与：月給28万円〜32万円');
    await expect(page.getByLabel('求人票')).toContainText('決まったルートで日用品を届けます。');
    await expect(page.getByLabel('求人票')).toContainText('月給28万円〜32万円');
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
      expect(sidebar!.height).toBeGreaterThanOrEqual(400);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
      expect(await page.evaluate(() => document.documentElement.scrollHeight - document.body.getBoundingClientRect().height)).toBeLessThan(2);
    }
    await page.evaluate(() => { window.scrollTo(0, 0); });
    await page.screenshot({ path: path.join(shots, `hrh-${width}.png`), fullPage: width === 390 });
    await selectJobFeature(page, 'timeline');
    await expect(jobFeaturePanel(page, 'timeline')).toContainText('月給28万');
    await selectJobFeature(page, 'diff');
    await expect(jobFeaturePanel(page, 'diff')).toContainText('月給25万円');
    await list.getByRole('button', { name: '看護スタッフ・大分31の版を見る' }).click();
    await expect(page.getByLabel('求人票')).toContainText('落ち着いた環境で利用者の健康を支える看護のお仕事です。');
    const currentImage = page.getByRole('region', { name: '現在取得できる掲載画像' }).locator('img');
    await expect(currentImage).toBeVisible();
    await expect.poll(() => currentImage.evaluate((element: HTMLImageElement) => element.naturalWidth)).toBe(800);
    await expect(page.getByText('この版の保存時点の画像は不明です。', { exact: false })).toBeVisible();
    await page.evaluate(() => { window.scrollTo(0, 0); });
    await page.screenshot({ path: path.join(shots, `airwork-${width}.png`), fullPage: width === 390 });
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
  await page.screenshot({ path: path.join(shots, 'preparing-1440.png') });
  await page.setViewportSize({ width: 1920, height: 1000 });
  await page.screenshot({ path: path.join(shots, 'preparing-1920.png') });
  await page.setViewportSize({ width: 390, height: 1000 });
  await page.screenshot({ path: path.join(shots, 'preparing-390.png'), fullPage: true });
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect(list.getByRole('button', { name: /の版を見る$/ })).toHaveCount(40, { timeout: 20_000 });
  await list.getByRole('button', { name: '看護スタッフ・大分37の版を見る' }).click();
  await expect(page.getByLabel('求人票')).toContainText('時給1800円');
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
  await expect(page.getByLabel('求人票')).toContainText('月給28万円〜32万円');
});
