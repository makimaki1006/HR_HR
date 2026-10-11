import { expect, test } from '@playwright/test';
import path from 'node:path';
const counts=(count:number)=>({count,first_at:null,last_at:null});
test('one card opens three messages with directions, counts, ordering and individual tracking intact', async ({page}) => {
  const requested:string[]=[];
  await page.route('**/api/crm/emails/*/tracking',async route=>{
    requested.push(new URL(route.request().url()).pathname);
    await route.fulfill({json:{opens:counts(route.request().url().includes('90001')?2:4),clicks:counts(1),automatic_opens:counts(0),automatic_clicks:counts(0),links:[]}});
  });
  await page.goto('/static/app/__thread_preview');
  await expect(page.locator('.wd-log-head')).toHaveText('1 件（メール 3 件）');
  await expect(page.getByTestId('activity-card')).toHaveCount(1);
  await expect(page.getByTestId('email-tracking-summary')).toHaveText('開封 4 回・クリック 1 回');
  if(process.env.SCREENSHOT_DIR) await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'thread-closed.png'),fullPage:true});
  await page.getByRole('button',{name:'メール',exact:true}).click();
  await expect(page.locator('.wd-log-head')).toHaveText('1 件（メール 3 件）');
  await page.getByRole('button',{name:'やり取り 3 件を開く'}).click();
  await expect(page.getByTestId('activity-card')).toHaveCount(3);
  await expect(page.getByTestId('activity-direction')).toHaveText(['送信','受信','送信']);
  await expect(page.getByTestId('email-tracking-summary')).toHaveText(['開封 4 回・クリック 1 回','開封 2 回・クリック 1 回']);
  await page.getByLabel('並び順').selectOption('oldest');
  await expect(page.getByTestId('activity-title')).toHaveText(['採用のご相談（架空）','Re: 採用のご相談（架空）','Re: 採用のご相談（架空）']);
  await expect(page.getByTestId('email-tracking-summary')).toHaveText(['開封 2 回・クリック 1 回','開封 4 回・クリック 1 回']);
  expect(requested).toEqual(['/api/crm/emails/90003/tracking','/api/crm/emails/90001/tracking']);
  if(process.env.SCREENSHOT_DIR) await page.screenshot({path:path.join(process.env.SCREENSHOT_DIR,'thread-open.png'),fullPage:true});
});
