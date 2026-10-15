import { openQueueToolbar } from './pr/helpers/crm_toolbar';
import {expect,test} from '@playwright/test';
import type {Page} from '@playwright/test';
import path from 'node:path';
const entry='/static/app/__views_preview';
const screenshot=async(page:Page,name:string)=>{if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,`views-${name}.png`),fullPage:true});};
/** ビューの機能は「詳細条件」で開く右のパネルの中の「ビュー」のまとまりにある */
const openPanel=async(page:Page)=>{
  await openQueueToolbar(page);
  const panel=page.getByRole('dialog',{name:'詳細条件'});
  if(!(await panel.isVisible()))await page.getByRole('button',{name:/^詳細条件/u}).click();
  await expect(panel).toBeVisible();
  return panel.getByTestId('saved-views');
};
const closePanel=async(page:Page)=>{await page.getByRole('button',{name:'条件のパネルを閉じる'}).click();};
test('the old saved-views row is gone from the screen; views live in the 詳細条件 panel',async({page})=>{
  await page.setViewportSize({width:1512,height:742});
  await page.goto(`${entry}?mode=fixture&owner=unassigned`);
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  // 行ごと無い: 「＋ ビューを保存」は画面に出ていない (パネルを開くまで見えない)
  await expect(page.getByRole('button',{name:'＋ ビューを保存',exact:true})).toBeHidden();
  await expect(page.getByRole('tablist',{name:'架電一覧のビュー'})).toHaveCount(0);
  const views=await openPanel(page);
  await expect(views.getByRole('heading',{name:'ビュー'})).toBeVisible();
  await expect(views.getByRole('button',{name:/^すべて/u})).toHaveAttribute('aria-pressed','true');
  await expect(views.getByRole('button',{name:'＋ ビューを保存',exact:true})).toBeVisible();
  await screenshot(page,'panel');
});
test('save, reopen and modify fictional views with the same exact queue count; menus and confirmation',async({page})=>{
  await page.setViewportSize({width:1440,height:960});
  const requests:string[]=[];page.on('request',r=>{if(new URL(r.url()).pathname.startsWith('/api/crm/'))requests.push(r.url());});
  await page.goto(`${entry}?mode=fixture&owner=unassigned`);
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  let views=await openPanel(page);
  await views.getByRole('button',{name:'＋ ビューを保存',exact:true}).click();
  await page.getByLabel('ビューの名前').fill('架空の未担当');
  await screenshot(page,'save-dialog');
  await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(views.getByRole('button',{name:'架空の未担当',exact:true})).toHaveAttribute('aria-pressed','true');
  await expect(page).toHaveURL(/view=[a-f0-9-]{36}/);
  // 開いているビューの名前が設定のバーに小さく出る
  await expect(page.getByTestId('current-view-name')).toHaveText('ビュー: 架空の未担当');
  await page.getByRole('combobox',{name:'所有者',exact:true}).selectOption('all');
  views=await openPanel(page);
  await expect(views.getByText('変更あり')).toBeVisible();
  await screenshot(page,'changed');
  // 別名で保存 (共有)
  await views.getByRole('button',{name:'別名で保存',exact:true}).click();await page.getByLabel('ビューの名前').fill('架空の共有一覧');
  await page.getByLabel('共有範囲').selectOption('shared');await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(views.getByRole('button',{name:'架空の共有一覧（全員に共有）'})).toHaveAttribute('aria-pressed','true');
  await screenshot(page,'list');
  // 保存済みのビューを開くと、その条件に戻る
  await views.getByRole('button',{name:'架空の未担当',exact:true}).click();
  await expect(page.getByRole('combobox',{name:'所有者',exact:true})).toHaveValue('unassigned');
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  // 変更して上書き保存、元に戻す
  await page.getByRole('combobox',{name:'所有者',exact:true}).selectOption('all');
  views=await openPanel(page);
  await views.getByRole('button',{name:'元に戻す',exact:true}).click();
  await expect(page.getByRole('combobox',{name:'所有者',exact:true})).toHaveValue('unassigned');
  await page.reload();await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  views=await openPanel(page);
  await views.getByLabel('架空の未担当のメニュー').click();await screenshot(page,'menu');
  await views.getByRole('button',{name:'下へ移す',exact:true}).click();
  await expect(views.getByRole('list',{name:'保存したビュー'}).locator('.sv-name')).toHaveText(['架空の共有一覧','架空の未担当']);
  await views.getByRole('button',{name:'削除',exact:true}).click();await expect(page.getByRole('dialog',{name:'ビューの削除確認'})).toBeVisible();
  await page.getByRole('button',{name:'やめる',exact:true}).click();await expect(views.getByRole('button',{name:'架空の未担当',exact:true})).toBeVisible();
  // 「すべて」に戻す
  await views.getByRole('button',{name:/^すべて/u}).click();await expect(page).not.toHaveURL(/view=/);await expect(page.getByTestId('current-view-name')).toHaveCount(0);
  await closePanel(page);
  expect(requests).toEqual([]);
});
test('a view-only bookmark restores filters; an invisible view returns to all with a notice',async({page})=>{
  await page.goto(`${entry}?mode=fixture&owner=unassigned`);
  let views=await openPanel(page);
  await views.getByRole('button',{name:'＋ ビューを保存',exact:true}).click();await page.getByLabel('ビューの名前').fill('架空の復元一覧');await page.getByRole('button',{name:'保存',exact:true}).click();
  await expect(page).toHaveURL(/view=[a-f0-9-]{36}/);const id=new URL(page.url()).searchParams.get('view');
  await page.goto(`${entry}?mode=fixture&view=${id}`);await expect(page).toHaveURL(/owner=unassigned/);await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
  await page.goto(`${entry}?mode=fixture&view=00000000-0000-4000-8000-000000000999`);
  await expect(page).not.toHaveURL(/view=/);
  views=await openPanel(page);
  await expect(views.getByText('このビューは見られないため、「すべて」に戻しました。')).toBeVisible();
  await expect(views.getByRole('button',{name:/^すべて/u})).toHaveAttribute('aria-pressed','true');await expect(page).not.toHaveURL(/view=/);
});

test('1024px: many views scroll inside the panel; no page overflow',async({page})=>{
  await page.setViewportSize({width:1024,height:800});
  await page.addInitScript(()=>{
    localStorage.setItem('hrhr.crm.fixtureSavedViews.v1',JSON.stringify(Array.from({length:16},(_,i)=>({
      id:`00000000-0000-4000-8000-${String(i+1).padStart(12,'0')}`,name:`架空の保存した一覧 ${String(i+1)}`,scope:i%2?'shared':'private',query:'owner=unassigned',position:i,created_at:'2026-10-12T00:00:00Z',updated_at:'2026-10-12T00:00:00Z',can_edit:true,
    }))));
  });
  await page.goto(`${entry}?mode=fixture&owner=all&view=00000000-0000-4000-8000-000000000001`);
  await expect(page.getByTestId('current-view-name')).toHaveText('ビュー: 架空の保存した一覧 1');
  const views=await openPanel(page);
  await expect(views.getByText('変更あり')).toBeVisible();await expect(views.locator('.sv-menu-toggle')).toHaveCount(1);
  await expect(views.getByRole('listitem')).toHaveCount(17);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await screenshot(page,'1024');
  await views.getByRole('button',{name:'架空の保存した一覧 16（全員に共有）'}).click();
  await expect(views.getByRole('button',{name:'架空の保存した一覧 16（全員に共有）'})).toHaveAttribute('aria-pressed','true');
  await expect(page.getByTestId('queue-count')).toContainText('全 2 件中 2 件');
});
