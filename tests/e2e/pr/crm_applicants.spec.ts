import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import * as fs from 'node:fs';
import * as path from 'node:path';

const sheetHtml = '<!doctype html><html lang="ja"><meta charset="utf-8"><title>架空の報告シート</title><body style="margin:0;font:12px sans-serif;color:#33475b"><div style="padding:12px;background:#eaf5ee">架空の報告シート（レイアウト確認用）</div><table style="border-collapse:collapse;width:100%">' + Array.from({length:24}, (_, i) => `<tr><td style="width:30px;padding:8px;border:1px solid #dfe3eb;background:#f5f7fa">${String(i+1)}</td><td style="padding:8px;border:1px solid #dfe3eb">${i===0 ? '確認項目' : 'サンプル項目'}</td><td style="padding:8px;border:1px solid #dfe3eb">${i===0 ? '報告欄' : ''}</td></tr>`).join('') + '</table></body></html>';

test('応募: 一覧から詳細、対応ルール、報告シート、取引モードへ戻る', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await login(page);
  const crmRequests: string[] = [];
  await page.route('**/api/crm/**', async route => { crmRequests.push(route.request().url()); await route.abort(); });
  await page.route('https://docs.google.com/**', route => route.fulfill({ contentType: 'text/html; charset=utf-8', body: sheetHtml }));
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
  await expect(detail.getByRole('heading', { name:'この求人の対応ルール' })).toBeInViewport();
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
  await page.route('https://docs.google.com/**', route => route.fulfill({ contentType: 'text/html; charset=utf-8', body: sheetHtml }));
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
  const bounds = await page.evaluate(() => {
    const center = document.querySelector('.applicants-center')!.getBoundingClientRect();
    const detail = document.querySelector('.applicants-detail')!.getBoundingClientRect();
    const editor = document.querySelector('.applicant-editor')!.getBoundingClientRect();
    const right = document.querySelector('.applicants-right')!.getBoundingClientRect();
    const frame = document.querySelector('iframe[title^="報告シート"]')!.getBoundingClientRect();
    return { centerHeight:center.height, detailHeight:detail.height, detailBottom:detail.bottom, editorHeight:editor.height, editorTop:editor.top, editorRight:editor.right, rightLeft:right.left, rightBottom:right.bottom, frameHeight:frame.height, frameBottom:frame.bottom };
  });
  expect(bounds.editorRight).toBeLessThanOrEqual(bounds.rightLeft);
  expect(bounds.editorHeight).toBeLessThanOrEqual(bounds.centerHeight / 2 + 1);
  expect(bounds.detailHeight).toBeGreaterThanOrEqual(bounds.centerHeight / 2 - 1);
  expect(Math.abs(bounds.detailBottom - bounds.editorTop)).toBeLessThan(2);
  expect(bounds.frameHeight).toBeGreaterThan(500);
  expect(bounds.rightBottom - bounds.frameBottom).toBeLessThanOrEqual(16);
  await expect(editor.getByRole('button', { name:'朝連絡', exact:true })).toBeInViewport();
  await expect(editor.getByRole('button', { name:'一次対応実施済', exact:true })).toBeInViewport();
  await expect(editor.getByRole('button', { name:'記録して次へ', exact:true })).toBeInViewport();
  await shot('05-result-input.png');
  await page.getByRole('main', { name: '応募の詳細' }).getByText('ヒアリングの編集', { exact: true }).click();
  await page.getByLabel('就業状況', { exact: true }).selectOption('consult');
  await page.getByLabel('関連業務経験', { exact: true }).fill('架空の検品作業の経験あり');
  await page.getByText('ヒアリングの編集', { exact:true }).evaluate(summary => {
    const pane = summary.closest('.applicants-detail')!;
    pane.scrollTop += summary.getBoundingClientRect().top - pane.getBoundingClientRect().top - 8;
  });
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
  await expect(page.getByText('いまは HubSpot に保存できない設定です。見るだけになります。')).toBeVisible();
  await expect(page.getByRole('button', { name: '記録して次へ', exact: true })).toHaveCount(0);
  await shot('08-writes-off.png');
});


test('応募: 長いステージ名もチェックボックスの右に一行で並ぶ', async ({ page }) => {
  await page.setViewportSize({ width:1280, height:800 });
  await login(page);
  await page.goto('/app/crm?mode=fixture&crmMode=applicants');
  await page.getByRole('button', { name:'ステージ（5件選択）' }).click();
  const panel = page.getByRole('group', { name:'ステージの選択' });
  for (const name of ['応募受付','足切り猶予期間','選考書類待ち','約束あり連絡']) {
    await expect(panel.getByRole('checkbox', { name, exact:true })).toBeVisible();
  }
  const rows = await panel.locator('li label').evaluateAll(labels => labels.map(label => {
    const input = label.querySelector('input')!.getBoundingClientRect();
    const span = label.querySelector('span')!.getBoundingClientRect();
    const range = document.createRange(); range.selectNodeContents(label.querySelector('span')!);
    return { width:input.width, inputRight:input.right, spanLeft:span.left, lineCount:range.getClientRects().length, inputCenter:input.top+input.height/2, textCenter:span.top+span.height/2 };
  }));
  expect(rows).toHaveLength(5);
  for (const row of rows) {
    expect(row.width).toBeLessThanOrEqual(20);
    expect(row.spanLeft).toBeGreaterThan(row.inputRight);
    expect(row.lineCount).toBe(1);
    expect(Math.abs(row.inputCenter-row.textCenter)).toBeLessThan(3);
  }
  if (process.env.CRM_APPLICANTS_ARTIFACTS) await page.screenshot({path:path.join(process.env.CRM_APPLICANTS_ARTIFACTS, '09-stage-filter.png')});
});
