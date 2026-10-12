import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

const scratch = process.env.A_QF2_SCREENSHOTS;
async function snap(page: import('@playwright/test').Page, name: string) {
  if (scratch) { await mkdir(scratch, { recursive:true }); await page.screenshot({path:`${scratch}/qf2-${name}.png`,fullPage:true}); }
}
test('架空一覧でまたはを加え、項目順を保存・復元し、上限の案内を表示する',async({page})=>{
  await page.setViewportSize({width:1440,height:900});await login(page);await page.goto('/app/crm?mode=fixture');
  await expect(page.getByText(/全 10 件中/)).toBeVisible();
  await page.getByRole('button',{name:'条件を追加',exact:true}).click();
  await page.getByLabel('項目',{exact:true}).selectOption('bpo_20');await page.getByLabel('条件',{exact:true}).selectOption('LAST_N_DAYS');await page.getByLabel('日数').fill('7');
  await snap(page,'relative');
  await page.getByLabel('項目',{exact:true}).selectOption('amount');await page.getByLabel('条件',{exact:true}).selectOption('GTE');await page.getByLabel('値',{exact:true}).fill('8000');await page.getByRole('button',{name:'この条件を追加',exact:true}).click();
  await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
  await page.getByRole('button',{name:'またはのまとまりを追加'}).click();
  await page.getByLabel('項目',{exact:true}).selectOption('amount');await page.getByLabel('条件',{exact:true}).selectOption('LTE');await page.getByLabel('値',{exact:true}).fill('2000');await page.getByRole('button',{name:'この条件を追加',exact:true}).click();
  await expect(page.getByText('全 5 件中 5 件を表示')).toBeVisible();await expect(page.getByRole('region',{name:'まとまり 2',exact:true})).toContainText('金額 以下 2000');
  const groups=JSON.parse(new URL(page.url()).searchParams.get('condition_groups')!);expect(groups).toEqual([[{property:'amount',operator:'GTE',values:['8000']}],[{property:'amount',operator:'LTE',values:['2000']}]]);
  await page.getByRole('button',{name:/詳細条件/}).click();await snap(page,'or');
  await page.reload();await expect(page.getByText('全 5 件中 5 件を表示')).toBeVisible();
  await page.getByRole('button',{name:'条件を追加',exact:true}).click();await page.getByLabel('並べ替える項目',{exact:true}).selectOption('amount');await page.getByLabel('順序',{exact:true}).selectOption('desc');await page.getByRole('button',{name:'この順に並べ替え'}).click();
  await expect(page.getByRole('list',{name:'架電キュー'}).locator('li').first()).toContainText('架空ホテル');
  await expect(page.getByText('選んだ項目の順に並べます。段階ごとの優先順は使わず、次回日の条件は絞り込みとして適用します。')).toBeVisible();
  expect(new URL(page.url()).searchParams.get('sort')).toBe('property:amount:desc');await page.getByRole('button',{name:/詳細条件/}).click();await snap(page,'sort');
  await page.reload();await expect(page.getByRole('list',{name:'架電キュー'}).locator('li').first()).toContainText('架空ホテル');
  await page.getByRole('button',{name:'条件をクリア',exact:true}).click();await page.getByRole('button',{name:'条件を追加',exact:true}).click();
  for(let i=0;i<5;i++){
    await page.getByLabel('項目',{exact:true}).selectOption('amount');await page.getByLabel('条件',{exact:true}).selectOption('GTE');await page.getByLabel('値',{exact:true}).fill('17');await page.getByRole('button',{name:'この条件を追加',exact:true}).click();
  }
  await expect(page.getByText('条件が多すぎます。条件かまとまりを減らしてください。')).toBeVisible();await page.getByRole('button',{name:/詳細条件/}).click();await snap(page,'limit');
});
