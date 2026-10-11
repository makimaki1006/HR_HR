import { expect, test } from '@playwright/test';
import type { Locator, Page } from '@playwright/test';
import path from 'node:path';
import { mkdir } from 'node:fs/promises';
async function visibleInViewport(page: Page, locator: Locator) {
  await expect(locator).toBeVisible();
  const box=await locator.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.y+box!.height).toBeLessThanOrEqual(page.viewportSize()!.height);
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x+box!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
}
async function open(page: Page) {
  await page.route('**/*',route => new URL(route.request().url()).hostname==='127.0.0.1' ? route.continue() : route.abort());
  await page.goto('/static/app/__compose_preview');
  await page.getByRole('list',{name:'架電キュー'}).locator('button.cq-row-button').first().click();
  await page.getByRole('button',{name:'メールを書く',exact:true}).click();
  return page.getByRole('dialog',{name:'メールの作成'});
}
async function shot(page: Page, name: string) {
  if (!process.env.A_COMPOSE_SCREENSHOTS) return;
  await mkdir(process.env.A_COMPOSE_SCREENSHOTS,{recursive:true});
  await page.screenshot({path:path.join(process.env.A_COMPOSE_SCREENSHOTS,`compose-${name}.png`)});
}
test('PC compose, twelve-second progress, successful email scroll and highlight',async ({page}) => {
  const dialog=await open(page);
  await dialog.getByLabel('件名').fill('採用計画についてのお打ち合わせ');
  await dialog.getByLabel('本文').fill('山田様\nお世話になっております。\n来週の火曜日14時から、お打ち合わせはいかがでしょうか。\n資料はこちらです。https://example.invalid/docs\nご検討よろしくお願いいたします。');
  await visibleInViewport(page,dialog.getByLabel('本文'));
  await visibleInViewport(page,dialog.getByRole('button',{name:'送信',exact:true}));
  await visibleInViewport(page,dialog.getByRole('button',{name:'閉じる'}));
  await visibleInViewport(page,dialog.locator('.crm-email-context'));
  await visibleInViewport(page,dialog.getByRole('button',{name:/に発信$/}));
  const overview=page.locator('.cq-detail');
  const dial=overview.locator('.wd-dial').first();
  await visibleInViewport(page,dial);
  expect(await dial.evaluate(el => { const r=el.getBoundingClientRect(); return el.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)); })).toBe(true);
  await shot(page,'open');
  const activity=page.getByTestId('activity-log');
  await activity.evaluate(el => { el.scrollTop=el.scrollHeight; });
  await dialog.getByRole('button',{name:'送信',exact:true}).click();
  await expect(dialog.getByText('経過 12 秒 ·')).toBeVisible({timeout:14000});
  await expect(dialog.getByRole('button',{name:'送信中…'})).toBeDisabled();
  await expect(dialog.getByRole('button',{name:'閉じる'})).toBeDisabled();
  await shot(page,'sending');
  await expect(dialog).toBeHidden({timeout:10000});
  await expect(activity.locator('[data-highlighted="true"]')).toContainText('採用計画についてのお打ち合わせ');
  await expect.poll(() => activity.evaluate(el => el.scrollTop)).toBe(0);
  await shot(page,'sent');
  await expect(activity.locator('[data-highlighted="true"]')).toHaveCount(0,{timeout:8000});
});
test('1366x768 editor retains visible actions and follows column width changes',async ({page}) => {
  await page.setViewportSize({width:1366,height:768});
  const dialog=await open(page);
  const separator=page.getByRole('separator').last();
  await separator.focus();
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await visibleInViewport(page,dialog.getByLabel('本文'));
  await visibleInViewport(page,dialog.getByRole('button',{name:'送信',exact:true}));
  await visibleInViewport(page,dialog.getByRole('button',{name:'閉じる'}));
  await visibleInViewport(page,dialog.locator('.crm-email-context'));
  await visibleInViewport(page,dialog.getByRole('button',{name:/に発信$/}));
  await dialog.getByLabel('件名').focus();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
});
