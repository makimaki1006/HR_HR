import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

const baseSummary = 'bpo_リクロジ · 架電対象だけ · 全員分 · 標準';
test.beforeEach(async ({ page }) => { await login(page); });

for (const size of [{ width: 1366, height: 768 }, { width: 1920, height: 1080 }]) {
  test(`${size.width}: 24pxの要約、300msの開閉、入力保持と固定。開いても一覧は動かない`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto('/app/crm?mode=fixture');
    await expect(page.getByTestId('queue-count')).toHaveText('全 10 件中 10 件を表示');
    const toggle = page.getByRole('button', { name: `条件を変更: ${baseSummary}` });
    const controls = page.locator('#cq-toolbar-controls');
    const list = page.getByRole('list', { name: '架電キュー' });
    const before = await list.boundingBox();
    expect((await toggle.boundingBox())?.height).toBe(24);
    await expect(controls).toBeHidden();
    const dir = process.env.SHOTS;
    if (dir) { await mkdir(dir, { recursive: true }); await page.screenshot({ path: `${dir}/${size.width}-closed.png` }); }
    await page.clock.install();
    await page.clock.pauseAt(new Date(Date.now() + 1000));
    await toggle.hover();
    await page.clock.runFor(299); await expect(controls).toBeHidden();
    await page.clock.runFor(1); await expect(controls).toBeVisible();
    expect((await list.boundingBox())?.y).toBe(before?.y);
    const panel = (await controls.boundingBox())!;
    expect(panel.x).toBe(16);
    expect(panel.width).toBe(size.width - 32);
    expect(panel.height).toBeGreaterThanOrEqual(220);
    const search = page.getByLabel('キーワード(会社名・案件名)');
    const sort = page.getByLabel('並び替え', { exact: true });
    const owner = page.getByLabel('所有者', { exact: true });
    for (const field of [search, sort, owner, page.getByLabel('対象', { exact: true }), page.getByRole('button', { name: 'パイプライン', exact: true })]) {
      expect((await field.boundingBox())!.height).toBe(36);
      await expect(field).toHaveCSS('font-size', '14px');
    }
    expect((await search.boundingBox())!.width).toBeGreaterThan(450);
    expect((await search.boundingBox())!.y).toBeGreaterThan((await owner.boundingBox())!.y + 36);
    // 選択中の標準の文言に、矢印と左右の余白を加えても収まる
    expect(await sort.evaluate(el => {
      const select = el as HTMLSelectElement;
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d')!;
      ctx.font = getComputedStyle(el).font;
      return el.clientWidth - ctx.measureText(select.selectedOptions[0].text).width;
    })).toBeGreaterThan(40);
    if (dir) await page.screenshot({ path: `${dir}/${size.width}-open.png` });
    await page.mouse.move(size.width - 10, size.height - 10);
    await page.clock.runFor(299); await expect(controls).toBeVisible();
    await page.clock.runFor(1); await expect(controls).toBeHidden();
    // キーボードで固定。タッチでも同じ button の click を使う
    await toggle.focus(); await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-pressed', 'true');
    await page.clock.runFor(600); await expect(controls).toBeVisible();
    await toggle.click(); await expect(controls).toBeHidden();
    await page.keyboard.press('Space'); await expect(controls).toBeVisible();
    await page.keyboard.press('Escape'); await expect(controls).toBeHidden();
    await expect(toggle).toBeFocused();
    // ホバーで開き、入力欄・select のフォーカス中は外れても閉じない
    await page.mouse.move(size.width - 10, size.height - 10);
    await toggle.hover(); await page.clock.runFor(300);
    for (const field of [page.getByLabel('キーワード(会社名・案件名)'), page.getByLabel('所有者', { exact: true })]) {
      await field.focus();
      await page.mouse.move(size.width - 10, size.height - 10);
      await page.clock.runFor(600); await expect(controls).toBeVisible();
    }
    await toggle.focus(); await page.clock.runFor(300); await expect(controls).toBeHidden();
    // 詳細条件は、外へフォーカスとポインタを動かしても保持する
    await toggle.hover(); await page.clock.runFor(300);
    await page.getByRole('button', { name: /^詳細条件/ }).click();
    await list.locator('.cq-row-button').first().focus();
    await page.mouse.move(size.width - 10, size.height - 10); await page.clock.runFor(600);
    await expect(page.getByRole('dialog', { name: '詳細条件', exact: true })).toBeVisible();
    await expect(controls).toBeVisible();
    // 入力した検索語を既存の条件更新で確定し、要約とURLで確認
    await page.getByRole('button', { name: '条件のパネルを閉じる' }).click();
    await page.clock.resume();
    await page.getByLabel('キーワード(会社名・案件名)').fill('架空');
    await expect.poll(() => new URL(page.url()).searchParams.get('q')).toBe('架空');
    await expect(page.getByTestId('queue-count')).toHaveText('全 7 件中 7 件を表示');
    await page.keyboard.press('Escape');
    await expect(controls).toBeHidden();
    await expect(page.locator('.cq-toolbar-summary')).toHaveAttribute('aria-label', `条件を変更: ${baseSummary} · 検索「架空」`);
    expect(new URL(page.url()).searchParams.get('q')).toBe('架空');
    await expect(page.getByTestId('queue-count')).toHaveText('全 7 件中 7 件を表示');
  });
}

test('URLの検索語・次回日・詳細条件数・担当と並び順を要約する', async ({ page }) => {
  const query = new URLSearchParams({ mode: 'fixture', q: '架空', owner: 'unassigned', due: 'today', sort: 'next_call_desc', next_from: '2026-01-01', last_to: '2026-10-15' });
  await page.goto(`/app/crm?${query}`);
  await expect(page.locator('.cq-toolbar-summary')).toHaveAttribute('aria-label', '条件を変更: bpo_リクロジ · 架電対象だけ · 担当者なし · 次回架電日が新しい順 · 検索「架空」 · 次回日が来たものだけ · 詳細条件 2');
  await expect(page.locator('#cq-toolbar-controls')).toBeHidden();
});

test('タッチ操作で固定して開き、もう一度押すと閉じる', async ({ browser }) => {
  const context = await browser.newContext({ hasTouch: true, viewport: { width: 390, height: 800 } });
  const page = await context.newPage();
  try {
    await login(page); await page.goto('/app/crm?mode=fixture');
    const toggle = page.locator('.cq-toolbar-summary');
    await toggle.tap(); await expect(toggle).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByLabel('キーワード(会社名・案件名)')).toBeVisible();
    await toggle.tap(); await expect(page.locator('#cq-toolbar-controls')).toBeHidden();
  } finally { await context.close(); }
});

for (const width of [1366, 1920]) {
  for (const period of ['today', 'month']) {
    test(`${width}: ${period}の主数字28px、成績は96pxの1段に収まる`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1080 });
      await page.goto(`/app/crm?mode=fixture${period === 'month' ? '&metrics=sales' : ''}`);
      const strip = page.getByTestId(`${period}-metrics`);
      await expect(strip.locator('.tm-value').first()).toHaveText(period === 'month' ? '812' : '52');
      await expect(strip.locator('.tm-value').first()).toHaveCSS('font-size', '28px');
      await expect(strip.locator('.tm-label').first()).toHaveCSS('font-size', '14px');
      await expect(strip.locator('.tm-head')).toHaveCSS('height', '96px');
      expect(await strip.locator('.tm-item').evaluateAll(items => new Set(items.map(el => Math.round(el.getBoundingClientRect().top))).size)).toBe(1);
      expect(await strip.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
      const last = (await strip.locator('.tm-item').last().boundingBox())!;
      const actions = (await strip.locator('.tm-actions').boundingBox())!;
      expect(last.x + last.width).toBeLessThan(actions.x);
      await strip.getByRole('button', { name: '閉じる', exact: true }).click();
      await expect(strip.locator('.tm-head')).toHaveCSS('height', '28px');
    });
  }
}
