import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

const shots = process.env.CRM_SCREENSHOT_DIR;
test.use({ viewport: { width: 1280, height: 800 } });

test.beforeEach(async ({ page }) => {
  await login(page);
  await page.route('https://applications.zoom.us/**', route => route.abort());
});

test('上段からステージを選択し、外へマウスを動かしても保持、適用した値をURLとチップに出す', async ({ page }) => {
  await page.goto('/app/crm?mode=fixture&pipeline=fx-sample');
  const summary = page.locator('.cq-toolbar-summary');
  const controls = page.locator('#cq-toolbar-controls');
  await summary.hover();
  await expect(controls).toBeVisible();
  await page.getByRole('button', { name: 'ステージ（2件選択）' }).click();
  const stages = page.getByRole('group', { name: 'ステージの選択' });
  await stages.getByLabel(/^新規/).uncheck();
  // 選択欄からフォーカスも外して、マウスだけで保持されることを確認
  await page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
  await page.mouse.move(1270, 790);
  await page.waitForTimeout(650);
  await expect(controls).toBeVisible();
  await expect(stages.getByLabel(/^追客/)).toBeChecked();
  if (shots) {
    await mkdir(shots, { recursive: true });
    await page.screenshot({ path: `${shots}/a-stage-selection.png` });
  }
  await stages.getByRole('button', { name: '適用', exact: true }).click();
  await expect.poll(() => new URL(page.url()).searchParams.getAll('stage')).toEqual(['fx-follow']);
  await expect(page.getByRole('list', { name: '適用中の条件' })).toContainText('ステージ: 追客');
  await page.getByRole('button', { name: 'ステージ（1件選択）' }).click();
  await page.keyboard.press('Escape');
  await expect(stages).toBeHidden();
  await expect(controls).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(controls).toBeHidden();
});

test('複数パイプライン・すべての取引でも上段で対象外ステージを選べる', async ({ page }) => {
  await page.goto('/app/crm?mode=fixture&pipelines=753186575,fx-sample&scope=all');
  await page.locator('.cq-toolbar-summary').click();
  await page.getByRole('button', { name: /^ステージ（/ }).click();
  const stages = page.getByRole('group', { name: 'ステージの選択' });
  await expect(stages.getByRole('heading')).toHaveCount(2);
  await stages.getByRole('button', { name: 'すべて外す' }).click();
  await stages.getByLabel('対象外', { exact: true }).check();
  await stages.getByRole('button', { name: '適用', exact: true }).click();
  await expect.poll(() => new URL(page.url()).searchParams.getAll('stage')).toEqual(['fx-stop']);
  await expect(page.getByRole('list', { name: '適用中の条件' })).toContainText('ステージ: 1 件を選択');
});

test('読み込み中の行と詳細・1行表示で同じ担当者名を出し、番号の箱には役職と関連ラベルも出す', async ({ page }) => {
  await page.goto('/app/crm?mode=fixture');
  const row = page.getByRole('list', { name: '架電キュー' }).locator('.cq-row-button').first();
  const contact = await row.locator('.cq-row-contact').evaluate(el => el.firstChild?.textContent?.trim());
  expect(contact).toBeTruthy();
  await row.click();
  const expand = page.getByRole('button', { name: '詳しく表示' });
  if (await expand.count()) await expand.click();
  const box = page.getByRole('region', { name: '架ける番号' });
  await expect(box.locator('.wd-dial-contact-name')).toHaveText(contact!);
  await expect(box.locator('.wd-dial-contact')).toContainText('採用担当');
  await expect(box.locator('.wd-dial-contact .cq-stage')).toHaveText('主');
  if (shots) {
    await mkdir(shots, { recursive: true });
    await page.screenshot({ path: `${shots}/b-contact-dialbox.png` });
  }
  await page.getByRole('button', { name: '1 行にする' }).click();
  await expect(page.getByTestId('head-line-1').locator('.wd-line-contact')).toHaveText(contact!);
});
