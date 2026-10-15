import { openQueueToolbar } from './helpers/crm_toolbar';
import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import { mkdir } from 'node:fs/promises';

test('追加条件で架電一覧と件数が変わり、URL復元・2条件・解除ができる', async ({ page }) => {
  await page.setViewportSize({ width:1440, height:900 });
  await login(page);
  await page.goto('/app/crm?mode=fixture');await openQueueToolbar(page);
  await expect(page.getByText(/全 10 件中/)).toBeVisible();
  await page.getByRole('button', { name:'条件を追加', exact:true }).click();
  await page.getByLabel('項目を検索', {exact:true}).fill('金額');
  await page.getByRole('option',{name:'金額',exact:true}).click();
  await page.getByLabel('条件', {exact:true}).selectOption('GTE');
  await page.getByLabel('値', {exact:true}).fill('8000');
  const dir = process.env.A_FILTER_SCREENSHOTS;
  if (dir) { await mkdir(dir,{recursive:true}); await page.screenshot({path:`${dir}/filter-add.png`,fullPage:true}); }
  await page.getByRole('button',{name:'この条件を追加',exact:true}).click();
  await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
  await expect(page.getByRole('list',{name:'適用中の条件'})).toContainText('金額 以上 8,000');
  expect(JSON.parse(new URL(page.url()).searchParams.get('conditions')!)).toEqual([{property:'amount',operator:'GTE',values:['8000']}]);
  await page.reload();await openQueueToolbar(page);
  await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
  await page.getByRole('button',{name:'条件を追加',exact:true}).click();
  await page.getByRole('button',{name:'＋ 条件を追加',exact:true}).click();
  await page.getByLabel('項目を検索',{exact:true}).fill('最終架電日');await page.getByRole('option',{name:'最終架電日',exact:true}).click();
  await page.getByLabel('条件',{exact:true}).selectOption('NOT_HAS_PROPERTY');
  await page.getByRole('button',{name:'この条件を追加',exact:true}).click();
  await expect(page.getByText('全 1 件中 1 件を表示')).toBeVisible();
  await expect(page.getByRole('list',{name:'架電キュー'})).toContainText('架空自動車整備');
  await page.getByRole('button',{name:/^詳細条件/}).click();
  if (dir) await page.screenshot({path:`${dir}/filter-two.png`,fullPage:true});
  await page.getByRole('button',{name:'「最終架電日 空」を外す'}).click();
  await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
});
