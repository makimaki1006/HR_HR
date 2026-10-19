import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import * as fs from 'node:fs';
import * as path from 'node:path';

const sheetHtml = '<!doctype html><html lang="ja"><meta charset="utf-8"><title>架空の報告シート</title><body style="margin:0;font:12px sans-serif;color:#33475b"><div style="padding:12px;background:#eaf5ee">架空の報告シート（レイアウト確認用）</div><table style="border-collapse:collapse;width:100%">' + Array.from({length:24}, (_, i) => `<tr><td style="padding:8px;border:1px solid #dfe3eb">${i+1}</td><td style="padding:8px;border:1px solid #dfe3eb">サンプル項目</td><td style="padding:8px;border:1px solid #dfe3eb">報告欄</td></tr>`).join('') + '</table></body></html>';
const postingHtml = '<!doctype html><html lang="ja"><meta charset="utf-8"><body style="font:16px sans-serif;color:#33475b;padding:24px"><p>架空サンプル求人票</p><h1>検品スタッフ</h1><h2>サンプル物流株式会社</h2><p>勤務地: 架空市サンプル町</p><p>勤務時間: 9:00〜17:00</p><p>未経験の方にも研修があります。</p></body></html>';

for (const size of [{ width:1512, height:742 }, { width:1280, height:800 }]) {
  test(`応募: Dock と求人票・報告シート・プロパティ ${size.width}x${size.height}`, async ({ page }) => {
    await page.setViewportSize(size);
    await login(page);
    const crmRequests: string[] = [];
    await page.route('**/api/crm/**', async route => { crmRequests.push(route.request().url()); await route.abort(); });
    await page.route('https://docs.google.com/**', route => route.fulfill({ contentType:'text/html; charset=utf-8', body:sheetHtml }));
    await page.route('https://www.example.com/**', route => route.fulfill({ contentType:'text/html; charset=utf-8', body:postingHtml }));
    await page.goto('/app/crm?mode=fixture&crmMode=applicants');
    await expect(page.getByText('2件', { exact:true })).toBeVisible();
    const firstRow = page.getByRole('button', { name:/架空 はるか/ });
    await expect(firstRow.locator('.cq-phone')).toHaveText('050-0000-0000');
    await firstRow.click();
    const detail = page.getByRole('main', { name:'応募の詳細' });
    const editor = page.getByRole('region', { name:'応募の記録' });
    await expect(detail.getByRole('heading', { level:2 })).toHaveText('架空 はるか');
    await expect(detail.getByRole('button', { name:/に発信$/ })).toBeDisabled();
    await expect(detail.getByRole('heading', { name:'この求人の対応ルール' })).toBeInViewport();
    await expect(page.getByRole('tab', { name:'求人票', exact:true })).toHaveAttribute('aria-selected','true');
    await expect(page.locator('iframe[title^="求人票"]')).toHaveAttribute('src','https://www.example.com/jobs/fictional-inspection');
    await expect(page.frameLocator('iframe[title^="求人票"]').getByRole('heading', { name:'検品スタッフ', exact:true })).toBeVisible();
    const dialBox = await detail.getByRole('button', { name:/に発信$/ }).boundingBox();
    expect(dialBox!.height).toBeLessThan(40);
    await expect(page.getByRole('link', { name:'案件管理で見る' })).toHaveAttribute('target','_blank');
    await expect(page.getByRole('tab', { name:'Zoom', exact:true })).toHaveCount(0);
    await expect(editor.getByRole('button', { name:'記録して次へ', exact:true })).toBeInViewport();
    const output = process.env.CRM_APPLICANTS_ARTIFACTS;
    async function shot(name: string) {
      if (output) { fs.mkdirSync(output, { recursive:true }); await page.screenshot({ path:path.join(output, `${size.width}x${size.height}-${name}.png`) }); }
    }
    await shot('01-posting');
    await page.getByRole('tab', { name:'報告シート', exact:true }).click();
    await expect(page.locator('iframe[title^="報告シート"]')).toHaveAttribute('src','https://docs.google.com/spreadsheets/d/fictional-applicants-sheet/edit');
    await expect(page.frameLocator('iframe[title^="報告シート"]').getByText('架空の報告シート（レイアウト確認用）')).toBeVisible();
    await shot('02-sheet');
    await page.getByRole('tab', { name:'プロパティ', exact:true }).click();
    const props = page.getByRole('complementary', { name:'応募のプロパティ' });
    await expect(props.getByText('カクウ ハルカ')).toBeVisible();
    await props.getByText('ヒアリングの編集', { exact:true }).click();
    await props.getByLabel('就業状況', { exact:true }).selectOption('consult');
    await props.getByLabel('関連業務経験', { exact:true }).fill('架空の検品作業の経験あり');
    await props.evaluate(el => { el.scrollTop = 0; });
    await shot('03-properties');
    await page.getByRole('tab', { name:'メモ', exact:true }).click();
    await expect(page.getByRole('region', { name:'応募のメモ' }).getByText('午後の連絡を希望しています。')).toBeVisible();
    await page.getByRole('tab', { name:'対応の入力', exact:true }).click();
    const divider = page.getByTestId('dock-divider-0-1');
    const before = await page.getByTestId('dock-col-0').boundingBox();
    const handle = await divider.boundingBox();
    if (!handle || !before) throw new Error('幅を変更する区切りがありません');
    await page.mouse.move(handle.x+handle.width/2, handle.y+handle.height/2);
    await page.mouse.down(); await page.mouse.move(handle.x+handle.width/2+35, handle.y+handle.height/2, { steps:5 }); await page.mouse.up();
    const after = await page.getByTestId('dock-col-0').boundingBox();
    expect(after!.width).toBeGreaterThan(before.width+20);
    await page.getByRole('tab', { name:'応募一覧', exact:true }).click();
    await editor.getByRole('button', { name:'昼連絡', exact:true }).click();
    await editor.getByLabel('対応種別', { exact:true }).selectOption('done');
    await editor.getByLabel('連絡・確認目的詳細', { exact:true }).fill('希望条件を確認しました');
    await expect(editor.getByLabel('ステージ', { exact:true })).toHaveValue('fixture-stage-0');
    await editor.getByRole('button', { name:'記録して次へ', exact:true }).click();
    await expect(detail.getByRole('heading', { level:2 })).toHaveText('架空 あおい');
    await expect(editor.locator('[data-state="saved"]')).toHaveText('✓ 保存済み');
    await expect(page.getByText('この応募には求人票の URL がありません。')).toBeVisible();
    await shot('04-no-posting');
    await page.getByRole('tab', { name:'報告シート', exact:true }).click();
    await expect(page.getByText('この応募には報告シートの URL がありません')).toBeVisible();
    expect(crmRequests).toEqual([]);
    await page.reload();
    expect((await page.getByTestId('dock-col-0').boundingBox())!.width).toBeCloseTo(after!.width, 0);
    await page.getByRole('link', { name:'取引', exact:true }).click();
    await expect(page.getByRole('list', { name:'架電キュー' })).toBeVisible();
    await page.getByRole('list', { name:'架電キュー' }).getByRole('button').first().click();
    await expect(page.getByTestId('center-tabs')).toBeVisible();
    await shot('05-deals');
    await page.goto('/app/crm?mode=fixture&crmMode=applicants&writes=off');
    await page.getByRole('button', { name:/架空 はるか/ }).click();
    await expect(page.getByText('いまは HubSpot に保存できない設定です。見るだけになります。')).toBeVisible();
    await expect(page.getByRole('button', { name:'記録して次へ', exact:true })).toHaveCount(0);
  });
}

test('応募: 長いステージ名もチェックボックスの右に一行で並ぶ', async ({ page }) => {
  await page.setViewportSize({ width:1280, height:800 });
  await login(page);
  await page.goto('/app/crm?mode=fixture&crmMode=applicants');
  await page.getByRole('button', { name:'条件を変更' }).click();
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
});
