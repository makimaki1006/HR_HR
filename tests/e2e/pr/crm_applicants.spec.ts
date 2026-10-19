import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import * as fs from 'node:fs';
import * as path from 'node:path';

test('応募: 一覧から詳細、対応ルール、報告シート、取引モードへ戻る', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await login(page);
  const crmRequests: string[] = [];
  await page.route('**/api/crm/**', async route => { crmRequests.push(route.request().url()); await route.abort(); });
  await page.route('https://docs.google.com/**', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>架空の報告シート</title>' }));
  await page.goto('/app/crm?mode=fixture&crmMode=applicants');
  await expect(page.getByText('2件', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: /架空 はるか/ }).click();
  const detail = page.getByRole('main', { name: '応募の詳細' });
  await expect(detail.getByRole('heading', { level: 2 })).toHaveText('架空 はるか');
  await expect(detail.getByText('カクウ ハルカ')).toBeVisible();
  await expect(detail.getByRole('button', { name: 'Zoom で発信' })).toBeDisabled();
  const output = process.env.CRM_APPLICANTS_ARTIFACTS;
  async function shot(name: string) {
    if (output) { fs.mkdirSync(output, { recursive: true }); await page.screenshot({ path: path.join(output, name) }); }
  }
  await expect(detail.getByRole('heading')).toHaveText(['架空 はるか', 'この求人の対応ルール', '応募先', 'メモ']);
  await shot('01-list-detail.png');
  await detail.getByText('ヒアリングと結果', { exact: true }).scrollIntoViewIfNeeded();
  await expect(detail.locator('dd').filter({ hasText: /^資格なし$/ })).toBeVisible();
  await detail.evaluate(pane => {
    const rules = pane.querySelector('h3');
    if (rules) pane.scrollTop += rules.getBoundingClientRect().top - pane.getBoundingClientRect().top - 12;
  });
  await expect(detail.getByRole('heading', { name: 'この求人の対応ルール' })).toBeInViewport();
  await expect(detail.locator('dd').filter({ hasText: /^資格なし$/ })).toBeVisible();
  await shot('02-rules-hearing.png');
  await page.getByRole('tab', { name: 'Zoom', exact: true }).click();
  await expect(page.getByText('架空サンプルでは発信できません')).toBeVisible();
  await page.getByRole('tab', { name: '報告シート', exact: true }).click();
  await expect(page.locator('iframe[title^="報告シート"]')).toHaveAttribute('src', 'https://docs.google.com/spreadsheets/d/fictional-applicants-sheet/edit');
  await expect(page.getByTestId('link-hint')).toHaveCount(0);
  await detail.evaluate(pane => { pane.scrollTop = 0; });
  await shot('03-report-sheet.png');
  await page.getByRole('button', { name: /架空 あおい/ }).click();
  await expect(page.getByText('この応募には報告シートの URL がありません')).toBeVisible();
  await shot('04-no-sheet.png');
  expect(crmRequests).toEqual([]);
  await page.getByRole('link', { name: '取引', exact: true }).click();
  await expect(page.getByRole('list', { name: '架電キュー' })).toBeVisible();
  expect(new URL(page.url()).searchParams.get('mode')).toBe('fixture');
  await expect(page.getByRole('link', { name: '取引', exact: true })).toHaveAttribute('aria-current', 'page');
});

test('応募: ステージと結果を記録して次の応募へ進む、OFF は読み取り', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await login(page);
  const crmRequests: string[] = [];
  await page.route('**/api/crm/**', async route => { crmRequests.push(route.request().url()); await route.abort(); });
  await page.route('https://docs.google.com/**', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>架空の報告シート</title>' }));
  const output = process.env.CRM_APPLICANTS_ARTIFACTS;
  async function shot(name: string) {
    if (output) { fs.mkdirSync(output, { recursive: true }); await page.screenshot({ path: path.join(output, name) }); }
  }
  await page.goto('/app/crm?mode=fixture&crmMode=applicants');
  await page.getByRole('button', { name: /架空 はるか/ }).click();
  const editor = page.getByRole('region', { name: '応募の記録' });
  await editor.getByRole('button', { name: '昼連絡', exact: true }).click();
  await editor.getByLabel('対応種別', { exact: true }).selectOption('done');
  await editor.getByLabel('連絡・確認目的詳細', { exact: true }).fill('希望条件を確認しました');
  await expect(editor.getByLabel('ステージ', { exact: true })).toHaveValue('fixture-stage-0');
  await shot('05-result-input.png');
  await page.getByRole('main', { name: '応募の詳細' }).getByText('ヒアリングの編集', { exact: true }).click();
  await page.getByLabel('就業状況', { exact: true }).selectOption('consult');
  await page.getByLabel('関連業務経験', { exact: true }).fill('架空の検品作業の経験あり');
  await shot('06-hearing-edit.png');
  await editor.getByRole('button', { name: '記録して次へ', exact: true }).click();
  await expect(page.getByRole('main', { name: '応募の詳細' }).getByRole('heading', { level: 2 })).toHaveText('架空 あおい');
  await expect(editor.locator('[data-state="saved"]')).toHaveText('✓ 保存済み');
  const saveBox = await editor.getByRole('button', { name: '記録して次へ', exact: true }).boundingBox();
  const editorBox = await editor.boundingBox();
  expect(saveBox && editorBox && saveBox.y + saveBox.height <= editorBox.y + editorBox.height).toBeTruthy();
  await editor.locator('[data-state="saved"]').scrollIntoViewIfNeeded();
  await shot('07-saved-green.png');
  await page.getByRole('button', { name: /架空 はるか/ }).click();
  await expect(editor.getByLabel('連絡・確認目的詳細', { exact: true })).toHaveValue('');
  expect(crmRequests).toEqual([]);
  await page.goto('/app/crm?mode=fixture&crmMode=applicants&writes=off');
  await page.getByRole('button', { name: /架空 はるか/ }).click();
  await expect(page.getByText('応募の書き込みは OFF のため、読み取りのみです。')).toBeVisible();
  await expect(page.getByRole('button', { name: '記録して次へ', exact: true })).toHaveCount(0);
  await shot('08-writes-off.png');
});
