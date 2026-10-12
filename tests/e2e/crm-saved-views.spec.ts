import {expect,test} from '@playwright/test';
import path from 'node:path';
const entry='/static/app/__views_preview';
const screenshot=async(page:import('@playwright/test').Page,name:string)=>{if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,`views-${name}.png`),fullPage:true});};
test('save, reopen and modify fictional views with the same exact queue count; menus and confirmation',async({page})=>{
  await page.setViewportSize({width:1440,height:960});
  const requests:string[]=[];page.on('request',r=>{if(new URL(r.url()).pathname.startsWith('/api/crm/'))requests.push(r.url());});
  await page.goto(`${entry}?mode=fixture&owner=unassigned`);
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  await page.getByRole('button',{name:'＋ ビューを保存',exact:true}).click();
  await page.getByLabel('ビューの名前').fill('架空の未担当');
  await screenshot(page,'save-dialog');
  await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(page.getByRole('tab',{name:'架空の未担当',exact:true})).toHaveAttribute('aria-selected','true');
  await expect(page).toHaveURL(/view=[a-f0-9-]{36}/);
  await page.getByRole('combobox',{name:'所有者',exact:true}).selectOption('all');
  await expect(page.getByText('変更あり')).toBeVisible();
  await screenshot(page,'changed');
  await page.getByRole('button',{name:'＋ ビューを保存',exact:true}).click();await page.getByLabel('ビューの名前').fill('架空の共有一覧');
  await page.getByLabel('共有範囲').selectOption('shared');await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(page.getByRole('tab',{name:'架空の共有一覧（全員に共有）'})).toHaveAttribute('aria-selected','true');
  await screenshot(page,'tabs');
  await page.getByRole('tab',{name:'架空の未担当',exact:true}).click();await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  await page.reload();await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  await page.getByLabel('架空の未担当のメニュー').click();await screenshot(page,'menu');
  await page.getByRole('button',{name:'右へ移す',exact:true}).click();
  await expect(page.getByRole('tablist',{name:'架電一覧のビュー'}).getByRole('tab')).toHaveText(['すべて','架空の共有一覧共有','架空の未担当']);
  await page.getByRole('button',{name:'削除',exact:true}).click();await expect(page.getByRole('dialog',{name:'ビューの削除確認'})).toBeVisible();
  await page.getByRole('button',{name:'やめる',exact:true}).click();await expect(page.getByRole('tab',{name:'架空の未担当'})).toBeVisible();
  expect(requests).toEqual([]);
});
test('a view-only bookmark restores filters; an invisible view returns to all with a notice',async({page})=>{
  await page.goto(`${entry}?mode=fixture&owner=unassigned`);
  await page.getByRole('button',{name:'＋ ビューを保存',exact:true}).click();await page.getByLabel('ビューの名前').fill('架空の復元一覧');await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(page).toHaveURL(/view=[a-f0-9-]{36}/);const id=new URL(page.url()).searchParams.get('view');
  await page.goto(`${entry}?mode=fixture&view=${id}`);await expect(page).toHaveURL(/owner=unassigned/);await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  await page.goto(`${entry}?mode=fixture&view=00000000-0000-4000-8000-000000000999`);
  await expect(page.getByText('このビューは見られないため、「すべて」に戻しました。')).toBeVisible();
  await expect(page.getByRole('tab',{name:'すべて',exact:true})).toHaveAttribute('aria-selected','true');await expect(page).not.toHaveURL(/view=/);
});

test('1024px: one full-width tab strip, overflow and only the selected menu',async({page})=>{
  await page.setViewportSize({width:1024,height:800});
  await page.addInitScript(()=>{
    localStorage.setItem('hrhr.crm.fixtureSavedViews.v1',JSON.stringify(Array.from({length:16},(_,i)=>({
      id:`00000000-0000-4000-8000-${String(i+1).padStart(12,'0')}`,name:`架空の保存した一覧 ${String(i+1)}`,scope:i%2?'shared':'private',query:'owner=unassigned',position:i,created_at:'2026-10-12T00:00:00Z',updated_at:'2026-10-12T00:00:00Z',can_edit:true,
    }))));
  });
  await page.goto(`${entry}?mode=fixture&owner=all&view=00000000-0000-4000-8000-000000000001`);
  const band=page.getByTestId('saved-views');await expect(band.getByRole('button',{name:'その他 ▾'})).toBeVisible();
  await expect(band.getByText('変更あり')).toBeVisible();await expect(band.locator('.sv-menu-toggle')).toHaveCount(1);
  const bounds=await band.boundingBox();const filters=await page.locator('.cq-filters').boundingBox();const head=await page.locator('.cq-list-head').boundingBox();
  expect(bounds).not.toBeNull();expect(filters).not.toBeNull();expect(head).not.toBeNull();
  expect(bounds!.width).toBe(1024);expect(bounds!.height).toBeLessThanOrEqual(46);expect(bounds!.y).toBeGreaterThanOrEqual(filters!.y+filters!.height);
  expect(head!.y).toBeGreaterThanOrEqual(bounds!.y+bounds!.height);
  for(const tab of await band.getByRole('tab').all()){const box=await tab.boundingBox();expect(box!.y).toBe(bounds!.y);expect(box!.x+box!.width).toBeLessThanOrEqual(1024);}
  const save=await band.getByRole('button',{name:'＋ ビューを保存',exact:true}).boundingBox();expect(save!.x+save!.width).toBeLessThanOrEqual(1024);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await screenshot(page,'1024');await band.getByRole('button',{name:'その他 ▾'}).click();
  await page.getByRole('group',{name:'その他のビュー'}).getByRole('button',{name:'架空の保存した一覧 16共有',exact:true}).click();
  await expect(page.getByRole('tab',{name:'架空の保存した一覧 16（全員に共有）'})).toHaveAttribute('aria-selected','true');
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
});
