import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';
const scratch = process.env.A_QF2_SCREENSHOTS;
async function snap(page: Page, name: string) {
  if (scratch) { await mkdir(scratch, {recursive:true}); await page.screenshot({path:`${scratch}/qf2-${name}.png`,fullPage:true}); }
}
async function pick(page:Page,label:string,input='項目を検索') { await page.getByRole('combobox',{name:input,exact:true}).fill(label);await page.getByRole('option',{name:label,exact:true}).click(); }
async function amount(page:Page,op:string,value:string) { await pick(page,'金額');await page.getByLabel('条件',{exact:true}).selectOption(op);await page.getByLabel('値',{exact:true}).fill(value);await page.getByRole('button',{name:'この条件を追加',exact:true}).click(); }
test('一覧を動かさず右パネルでまたはを編集し、並べ替えと上限・1024幅を確認する',async({page})=>{
  await page.setViewportSize({width:1440,height:900});await login(page);await page.goto('/app/crm?mode=fixture');
  await expect(page.getByText(/全 10 件中/)).toBeVisible();
  const list=page.getByRole('list',{name:'架電キュー'});const before=await list.boundingBox();
  await page.getByRole('button',{name:'条件を追加',exact:true}).click();
  await pick(page,'最終架電日');await page.getByLabel('条件',{exact:true}).selectOption('LAST_N_DAYS');await page.getByLabel('日数').fill('7');
  expect((await list.boundingBox())?.y).toBe(before?.y);await snap(page,'relative');
  await amount(page,'GTE','8000');await expect(page.getByText('全 3 件中 3 件を表示')).toBeVisible();
  await page.getByRole('button',{name:'＋ 「または」の条件を追加'}).click();await amount(page,'LTE','2000');
  await expect(page.getByText('全 5 件中 5 件を表示')).toBeVisible();await expect(page.getByRole('region',{name:'まとまり 2',exact:true})).toContainText('金額 以下 2,000');await snap(page,'or');
  expect(JSON.parse(new URL(page.url()).searchParams.get('condition_groups')!)).toEqual([[{property:'amount',operator:'GTE',values:['8000']}],[{property:'amount',operator:'LTE',values:['2000']}]]);
  await page.setViewportSize({width:1024,height:900});
  const box=await list.boundingBox();const drawer=await page.getByRole('dialog',{name:'詳細条件',exact:true}).boundingBox();
  expect(box).not.toBeNull();expect(drawer).not.toBeNull();expect(box!.x+box!.width).toBeLessThanOrEqual(drawer!.x);await snap(page,'1024');
  await page.setViewportSize({width:1440,height:900});await page.getByRole('button',{name:'条件のパネルを閉じる'}).click();await page.reload();await expect(page.getByText('全 5 件中 5 件を表示')).toBeVisible();
  await page.getByLabel('並び替え',{exact:true}).selectOption('other');await expect(page.getByRole('dialog',{name:'ほかの項目で並べ替え'})).toBeVisible();
  await page.keyboard.press('Escape');await expect(page.getByRole('dialog',{name:'ほかの項目で並べ替え'})).toHaveCount(0);
  await expect(page.getByLabel('並び替え',{exact:true})).toHaveValue('default');await page.getByLabel('並び替え',{exact:true}).selectOption('other');
  await pick(page,'金額','並べ替える項目を検索');await page.getByLabel('順序',{exact:true}).selectOption('desc');await page.getByRole('button',{name:'この順に並べ替え'}).click();
  await expect(list.locator('li').first()).toContainText('架空ホテル');await expect(page.getByLabel('並び替え',{exact:true})).toHaveValue('property:amount:desc');await snap(page,'sort');
  await page.reload();await expect(list.locator('li').first()).toContainText('架空ホテル');await page.getByRole('button',{name:'条件をクリア',exact:true}).click();
  await page.getByRole('button',{name:'条件を追加',exact:true}).click();
  for(let i=0;i<3;i++) { if(i>0) await page.getByRole('button',{name:'＋ 条件を追加',exact:true}).click();await amount(page,'GTE','17'); }
  await expect(page.getByText('条件はあと 0 個です')).toBeVisible();await expect(page.getByRole('button',{name:'＋ 条件を追加',exact:true})).toBeDisabled();
  await expect(page.getByRole('alert')).toHaveCount(0);await expect(page.getByRole('button',{name:'再試行'})).toHaveCount(0);await snap(page,'limit');
});
test('保存された上限超過の条件でも再試行を出さない',async({page})=>{
  await login(page);const conditions=Array.from({length:5},()=>({property:'amount',operator:'GTE',values:['17']}));
  await page.goto('/app/crm?mode=fixture&conditions='+encodeURIComponent(JSON.stringify(conditions)));
  await expect(page.getByText('条件が多すぎます。条件かまとまりを減らしてください。')).toBeVisible();await expect(page.getByRole('button',{name:'再試行'})).toHaveCount(0);
});
