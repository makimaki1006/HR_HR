import {expect,test} from '@playwright/test';
import path from 'node:path';
const shot=async(page:import('@playwright/test').Page,name:string)=>{if(process.env.SCREENSHOT_DIR)await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,name),fullPage:false});};
test('appointer strip shows own values with average and rank; collapses; no live requests in fixture mode',async({page})=>{
  const requests:string[]=[];page.on('request',r=>{if(new URL(r.url()).pathname.startsWith('/api/crm/'))requests.push(r.url());});
  await page.goto('/static/app/__metrics_crm?mode=fixture&owner=unassigned');
  const strip=page.getByTestId('today-metrics');
  await expect(strip).toBeVisible();
  await expect(page.getByTestId('metrics-as-of')).toHaveText('14時05分時点');
  await expect(page.getByTestId('metric-outbound')).toContainText('52');
  await expect(page.getByTestId('metric-outbound')).toContainText('チーム平均 47.5');
  await expect(page.getByTestId('metric-outbound')).toContainText('3位/8人');
  await expect(page.getByTestId('metric-over_90s')).toContainText('2位/8人');
  await expect(page.getByTestId('metric-duration')).toContainText('55:12・1件 2:18');
  await expect(page.getByTestId('metric-appointments')).toContainText('1位/8人');
  const box=await strip.boundingBox();expect(box!.height).toBeLessThanOrEqual(32); // 1 行のまま (数字を大きくした分 +8px まで)
  await shot(page,'metrics-strip.png');
  await page.getByRole('button',{name:'閉じる'}).click();
  await expect(page.getByTestId('metric-outbound')).toHaveCount(0);
  expect(requests.filter(u=>u.includes('/metrics/'))).toEqual([]);
});
test('admin table sorts and filters by team (fictional)',async({page})=>{
  await page.goto('/static/app/__metrics_admin?view=callers&mode=fixture');
  const rows=page.getByTestId('callers-table').locator('tbody tr');
  await expect(rows).toHaveCount(5);
  await expect(rows.first().locator('td').first()).toHaveText('サンプル 二郎');
  await page.getByRole('button',{name:/アポ/}).click();
  await expect(rows.first().locator('td').first()).toHaveText('架空 花子');
  await shot(page,'metrics-admin.png');
  await page.getByLabel('チームで絞り込む').selectOption('架空営業チーム');
  await expect(page.getByTestId('callers-count')).toHaveText('1 人');
});
test('1024px: the strip wraps without overflowing or hiding the toggle',async({page})=>{
  await page.setViewportSize({width:1024,height:800});
  await page.goto('/static/app/__metrics_crm?mode=fixture&owner=unassigned');
  const strip=page.getByTestId('today-metrics');await expect(strip).toBeVisible();
  const box=await strip.boundingBox();expect(box!.width).toBe(1024);expect(box!.height).toBeLessThanOrEqual(64);
  for(const li of await strip.locator('.tm-item').all()){const b=await li.boundingBox();expect(b!.x+b!.width).toBeLessThanOrEqual(1024);}
  const t=await strip.getByRole('button',{name:'閉じる'}).boundingBox();expect(t!.x+t!.width).toBeLessThanOrEqual(1024);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await expect(strip.locator('.tm-cmp').first()).toHaveText('（チーム平均 47.5・3位/8人）');
});
for(const w of [1366,1280]){
  test(`${w}px: the strip stays on one line with the short comparison`,async({page})=>{
    await page.setViewportSize({width:w,height:768});
    await page.goto('/static/app/__metrics_crm?mode=fixture&owner=unassigned');
    const strip=page.getByTestId('today-metrics');await expect(strip).toBeVisible();
    const box=await strip.boundingBox();expect(box!.height).toBeLessThanOrEqual(32);
    const cmp=strip.locator('.tm-cmp').first();
    expect(await cmp.evaluate(e=>getComputedStyle(e,'::after').content)).toBe('"（平均47.5・3位/8）"');
    await expect(cmp).toHaveText('（チーム平均 47.5・3位/8人）');
    await expect(page.getByTestId('metric-outbound')).toHaveAttribute('title',/チーム平均 47\.5・3位\/8人/);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
    await shot(page,`metrics-strip-${w}.png`);
  });
}
test('1512px: custom items sit in the strip like the built-in ones; long strips shorten, then wrap instead of scrolling',async({page})=>{
  await page.setViewportSize({width:1512,height:742});
  await page.goto('/static/app/__metrics_crm?mode=fixture&owner=unassigned&metrics=custom');
  const strip=page.getByTestId('today-metrics');await expect(strip).toBeVisible();
  await expect(page.getByTestId('metric-c-absent')).toContainText('不在');
  await expect(page.getByTestId('metric-c-absent')).toContainText(/位\/8/);
  await expect(page.getByTestId('metric-c-amount')).toContainText(/今月の金額[\d,]+/);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  for(const li of await strip.locator('.tm-item').all()){const b=await li.boundingBox();expect(b!.x+b!.width).toBeLessThanOrEqual(1512);}
  const box=await strip.boundingBox();expect(box!.height).toBeLessThanOrEqual(64);
  await shot(page,'strip-custom.png');
  await page.getByRole('button',{name:'⚙ 項目'}).click();
  const panel=page.getByTestId('metrics-settings');await expect(panel).toBeVisible();
  await expect(panel.getByTestId('metric-setting-c-amount')).toContainText('自分の項目・合計');
  await shot(page,'strip-settings.png');
  await panel.getByRole('button',{name:'直す'}).last().click();
  await expect(panel.getByLabel('名前')).toHaveValue('今月の金額');
  await panel.getByRole('button',{name:'この項目を決める'}).scrollIntoViewIfNeeded();
  await shot(page,'strip-settings-editor.png');
  expect((await panel.boundingBox())!.x).toBeGreaterThanOrEqual(0);
});
test('settings: hide an item and save; the strip follows the saved order (fixture, no requests)',async({page})=>{
  const requests:string[]=[];page.on('request',r=>{if(new URL(r.url()).pathname.startsWith('/api/crm/metrics'))requests.push(r.url());});
  await page.goto('/static/app/__metrics_crm?mode=fixture&owner=unassigned');
  await page.getByRole('button',{name:'⚙ 項目'}).click();
  const panel=page.getByTestId('metrics-settings');
  await panel.getByRole('checkbox',{name:'アポを帯に出す'}).uncheck();
  await panel.getByRole('button',{name:'通話時間を上へ'}).click();
  await panel.getByRole('button',{name:'保存'}).click();
  await expect(panel.getByText('保存しました')).toBeVisible();
  await expect(page.getByTestId('metric-appointments')).toHaveCount(0);
  const keys=await page.getByTestId('today-metrics').locator('.tm-item').evaluateAll(ls=>ls.map(l=>l.getAttribute('data-testid')));
  expect(keys).toEqual(['metric-outbound','metric-duration','metric-over_90s']);
  expect(requests).toEqual([]);
});
