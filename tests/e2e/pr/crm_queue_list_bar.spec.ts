import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

// 一覧の行・件数の行・詳細条件のパネルの使い勝手 (架空の fixture)。SHOTS=保存先 を付けるとスクリーンショットも撮る
const rows = (page: Page) => page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row');
async function open(page: Page, w: number, h: number) {
  await page.setViewportSize({ width: w, height: h });
  await login(page);
  await page.goto('/app/crm?mode=fixture');
  await expect(rows(page).first()).toBeVisible();
}
async function shot(page: Page, name: string) {
  const dir = process.env.SHOTS;
  if (!dir) return;
  await mkdir(dir, { recursive: true });
  await page.screenshot({ path: `${dir}/${name}.png` });
}

test('記録済みの印は短く、詳しい説明は title に。選んでいる行の Enter でステージのボタンへ。⚠ の印を押すと内訳が開く', async ({ page }) => {
  await open(page, 1512, 742);
  await rows(page).first().locator('button.cq-row-button').click();
  await page.getByRole('tab', { name: '架電結果の入力' }).click();
  await page.getByRole('form', { name: '架電結果の入力' }).getByRole('button', { name: '受付ブロック', exact: true }).click();
  await page.getByRole('button', { name: /記録して次へ/ }).click();
  const mark = rows(page).first().locator('.cq-recorded');
  await expect(mark.locator('[aria-hidden="true"]')).toHaveText('✓記録済');
  await expect(mark).toHaveAttribute('title', /HubSpot に保存しました/);
  const box = await mark.boundingBox();
  expect(box?.width ?? 999).toBeLessThan(70);
  // 会社名が切れない
  const company = rows(page).first().locator('.cq-row-company');
  expect(await company.evaluate(e => e.scrollWidth <= e.clientWidth)).toBe(true);

  // ⚠ の印は件数の行の右端、押すと内訳が開く
  const notes = page.getByTestId('queue-notes');
  await expect(notes.locator('summary')).toHaveText(/^⚠\d+$/);
  await expect(notes.locator('.cq-notes-pop')).toBeHidden();
  await shot(page, 'g3-list-1512');
  await page.setViewportSize({ width: 1366, height: 768 });
  await shot(page, 'g3-list-1366');
  await notes.locator('summary').click();
  await expect(notes.locator('.cq-notes-pop')).toBeVisible();
  await expect(notes.locator('.cq-notes-pop')).toContainText('一部の情報が欠けています');
  await shot(page, 'g3-notes-open');
  await notes.locator('summary').click();

  // 選んでいる行で Enter → ステージのボタンにフォーカス (別のパネルが前に出ていても前に出す)
  await page.getByRole('tab', { name: '活動ログ' }).click();
  await page.locator('button.cq-row-button[aria-pressed="true"]').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('form[aria-label="架電結果の入力"] .rf-stage-buttons button:focus')).toHaveCount(1);
});

test('詳細条件は外側のクリックと Esc で閉じ、パネルの中では閉じない。「条件を追加」は検索欄へフォーカスする', async ({ page }) => {
  await open(page, 1512, 742);
  const panel = page.getByRole('dialog', { name: '詳細条件', exact: true });
  await page.getByRole('button', { name: /^詳細条件/ }).click();
  await expect(panel).toBeVisible();
  await expect(page.getByLabel('項目を検索', { exact: true })).toHaveCount(0);

  // カレンダー (body 直下) の中のクリックでは閉じない
  await panel.locator('button.dp-open').first().click();
  const pop = page.getByTestId('dp-popover');
  await expect(pop).toBeVisible();
  await pop.click({ position: { x: 3, y: 3 } });
  await expect(panel).toBeVisible();
  await panel.locator('button.dp-open').first().click();
  await expect(pop).toBeHidden();
  await expect(panel).toBeVisible();

  // 外側のクリックで閉じる
  await page.locator('.cq-list-head').click({ position: { x: 5, y: 5 } });
  await expect(panel).toBeHidden();

  // 「条件を追加」: 開いて検索欄にフォーカス。条件は追加時に URL へ入るので、閉じても失われない
  await page.getByRole('button', { name: '条件を追加', exact: true }).click();
  await expect(panel).toBeVisible();
  await expect(page.getByLabel('項目を検索', { exact: true })).toBeFocused();
  await shot(page, 'g3-add');
  await page.getByLabel('項目を検索', { exact: true }).fill('金額');
  await page.getByRole('option', { name: '金額', exact: true }).click();
  await page.getByLabel('条件', { exact: true }).selectOption('GTE');
  await page.getByLabel('値', { exact: true }).fill('8000');
  await page.getByRole('button', { name: 'この条件を追加', exact: true }).click();
  await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
  // Esc で閉じる
  await page.keyboard.press('Escape');
  await expect(panel).toBeHidden();
  await expect(page.getByRole('list', { name: '適用中の条件' })).toContainText('金額 以上 8,000');
});
