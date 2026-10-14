import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { startWithResultPanelInFront } from './helpers/crm_layout';

/**
 * 架電画面 (/app/crm?mode=fixture) のキーボード操作・画面の高さ別の見え方・画面の文言の E2E。
 *
 * - 架空サンプルで動かす (/api/crm/* は呼ばない)。値は frontend/src/screens/crm/queueFixture.ts / workspaceFixture.ts の固定値。
 * - この画面にグラフ (ECharts) は無い。値は描画された文字・計算済みのスタイル・要素の位置と sessionStorage で確かめる。
 * - 位置は getBoundingClientRect と、スクロールする親の見えている範囲を比べる (boundingBox だけでは親の中で隠れていても通るため)。
 */

const SCREEN_URL = '/app/crm?mode=fixture';
const STORAGE_KEY = 'hrhr.crm.callResult.v2';

const list = (page: Page) => page.getByRole('list', { name: '架電キュー' });
const rowButton = (page: Page, i: number) => list(page).locator('li.cq-row').nth(i).locator('button.cq-row-button');
const form = (page: Page) => page.getByRole('form', { name: '架電結果の入力' });
const stages = (page: Page) => form(page).getByRole('group', { name: 'ステージ(よく使うもの)' });
const stage = (page: Page, label: string) => stages(page).getByRole('button', { name: label, exact: true });
const article = (page: Page) => page.getByRole('article', { name: '架電先の詳細' });

async function openFirst(page: Page): Promise<void> {
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await expect(list(page).locator('.cq-row-company').first()).toHaveText('ダミー建設');
  await rowButton(page, 0).click();
  await expect(article(page).locator('h2')).toHaveText('ダミー建設');
  // BPO の組: よく使うステージのボタン (架空サンプルのパイプラインにあるもの)。今のステージ (担当者ブロック) は押せない
  await expect(stages(page).getByRole('button')).toHaveText(['不在', '受付ブロック', '不通要入力', '担当者ブロック', '日程確保', 'アポ日確定要入力']);
  await expect(stage(page, '担当者ブロック')).toBeDisabled();
}

async function recordedMap(page: Page): Promise<Record<string, boolean>> {
  return page.evaluate((k) => (JSON.parse(window.sessionStorage.getItem(k) ?? '{"recorded":{}}') as { recorded: Record<string, boolean> }).recorded, STORAGE_KEY);
}

/** 架ける番号・発信ボタンが、詳細の上端 (.wd-top) の中で切れずに見えているか */
async function dialVisibility(page: Page) {
  return page.evaluate(() => {
    const r = (sel: string) => {
      const el = document.querySelector(sel);
      if (!el) throw new Error(`missing ${sel}`);
      const b = el.getBoundingClientRect();
      return { top: b.top, bottom: b.bottom };
    };
    const top = document.querySelector<HTMLElement>('.wd-top');
    if (!top) throw new Error('missing .wd-top');
    return {
      topClient: top.clientHeight, topScroll: top.scrollHeight, top: r('.wd-top'),
      number: r('.wd-phone-primary .cq-phone'), dial: r('.wd-phone-primary .wd-dial'),
      numberText: document.querySelector('.wd-phone-primary .cq-phone')?.textContent ?? '',
    };
  });
}

test.describe('CRM 架電画面: キーボード・画面の高さ・文言', () => {
  // 架電結果の入力欄を確かめるので、「架電結果の入力」を前に出した配置で始める (既定は「活動ログ」が前)
  test.beforeEach(async ({ page }) => { await startWithResultPanelInFront(page); await login(page); });

  test('1 行の入力欄・日付欄で Enter を押しても記録しない (Ctrl+Enter だけが記録する)', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);
    const count = form(page).getByLabel('日程切り', { exact: true });
    await count.fill('2');
    await count.press('Enter');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', 'f-5');
    await expect(rowButton(page, 0)).toHaveAttribute('aria-pressed', 'true');
    expect(await recordedMap(page)).toEqual({});

    const tomorrow = new Date(Date.now() + 9 * 3600_000 + 86_400_000).toISOString().slice(0, 10);
    const date = form(page).getByLabel('次回架電日', { exact: true });
    await date.fill(tomorrow.replaceAll('-', '/'));
    await form(page).getByLabel('次回架電時間', { exact: true }).selectOption('9:15');
    await date.focus();
    await date.press('Enter');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', 'f-5');
    expect(await recordedMap(page)).toEqual({});

    // Ctrl+Enter は記録する (HubSpot に保存。架空サンプルでは画面の中だけ)
    await date.press('Control+Enter');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', 'f-2');
    expect(await recordedMap(page)).toEqual({ 'fixture:f-5': true });
  });

  for (const vp of [{ width: 1280, height: 720 }, { width: 1366, height: 768 }, { width: 1440, height: 900 }]) {
    test(`${vp.width}x${vp.height}: 「架ける番号」と発信ボタンは詳細の上端で切れずに見える (結果を選んだ後も)`, async ({ page }) => {
      await page.setViewportSize(vp);
      await openFirst(page);
      for (const step of ['before', 'after'] as const) {
        if (step === 'after') {
          await stage(page, '受付ブロック').click();
          await expect(form(page).getByRole('textbox', { name: 'タスクメモに追記' })).toBeVisible();
        }
        const v = await dialVisibility(page);
        expect(v.numberText, step).toBe('03-0000-0005');
        // 上端は自分の中でスクロールしない (中身が全部見えている)
        expect(v.topScroll, `${step} ${JSON.stringify(v)}`).toBeLessThanOrEqual(v.topClient + 1);
        // 番号と発信ボタンは「案件の概要」(列の上端に固定) の中に収まる
        expect(v.number.top, step).toBeGreaterThanOrEqual(v.top.top);
        expect(v.number.bottom, step).toBeLessThanOrEqual(v.top.bottom);
        expect(v.dial.bottom, step).toBeLessThanOrEqual(v.top.bottom);
        expect(v.dial.bottom - v.dial.top, step).toBeGreaterThan(20);
        await expect(article(page).locator('.wd-phone-primary .wd-dial')).toBeInViewport({ ratio: 1 });
      }
      // 記録ボタンも見えている
      await expect(form(page).getByRole('button', { name: /記録して次へ/ })).toBeInViewport({ ratio: 1 });
    });
  }

  test('キーボードだけで続けて記録できる: 記録の後は次の案件のステージのボタンへ、Space でステージを選び、Ctrl+Enter で記録', async ({ page }) => {
    await page.setViewportSize({ width: 1366, height: 768 });
    await openFirst(page);
    await stage(page, '不在').focus();
    await page.keyboard.press('Space');
    await expect(stage(page, '不在')).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.press('Control+Enter');
    // 次の案件 (架空食品株式会社) のステージのボタン (今のステージ「不在」は押せないので、その次) にフォーカス。読み上げ欄に保存した会社
    await expect(article(page).locator('h2')).toHaveText('架空食品株式会社');
    await expect(rowButton(page, 1)).toHaveAttribute('aria-pressed', 'true');
    await expect(stage(page, '受付ブロック')).toBeFocused();
    await expect(page.getByTestId('screen-announcement')).toHaveText(/^ダミー建設 を HubSpot に保存しました。次の架電先を表示しています。\s?$/);
    await expect(list(page).locator('li.cq-row').nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み)');
    await expect(form(page).getByTestId('result-notice')).toHaveText('前の架電先: ダミー建設 を HubSpot に保存しました。');

    // そのまま次も: Space、Ctrl+Enter
    await page.keyboard.press('Space'); // 受付ブロック
    await page.keyboard.press('Control+Enter');
    await expect(article(page).locator('h2')).toHaveText('架空ホテル');
    expect(await recordedMap(page)).toEqual({ 'fixture:f-5': true, 'fixture:f-2': true });
  });

  test('折りたたみ中の Ctrl+Enter は記録せず、入力欄を開いて知らせる', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);
    await stage(page, '不在').click();
    const toggle = form(page).getByRole('button', { name: /架電結果/ });
    await toggle.focus();
    await page.keyboard.press('Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    const bodyId = await toggle.getAttribute('aria-controls');
    expect(bodyId).toBeTruthy();
    await expect(page.locator(`[id="${bodyId}"]`)).toBeHidden();
    await expect(form(page).getByTestId('draft-summary')).toHaveText('ステージ: 不在');
    await page.keyboard.press('Control+Enter');
    await expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await expect(form(page).locator('.rf-notice')).toHaveText('入力欄を開きました。内容を確かめてから、もう一度「記録して次へ」を押してください。');
    expect(await recordedMap(page)).toEqual({});
  });

  test('フォーカスの枠と小さい注記の色は白地で十分なコントラスト (枠 #0d7680 / 注記 #5b6b7a)', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);
    await expect(form(page).locator('.rf-hint').first()).toHaveText('書いた内容は、今の内容の末尾に日付を付けて足します。全文を直すときは、左のプロパティ欄の編集を使ってください。');
    expect(await form(page).locator('.rf-hint').first().evaluate((e) => getComputedStyle(e).color)).toBe('rgb(91, 107, 122)');
    expect(await form(page).locator('.rf-sub').first().evaluate((e) => getComputedStyle(e).color)).toBe('rgb(91, 107, 122)');
    // キーボードでフォーカス (focus-visible) したステージのボタン・記録ボタンの枠
    await stage(page, '不在').focus();
    await page.keyboard.press('Tab');
    const ring = (sel: string) => page.locator(sel).evaluate((e) => { const s = getComputedStyle(e); return `${s.outlineColor} ${s.outlineStyle}`; });
    expect(await ring('.rf-stage-btn:focus')).toBe('rgb(13, 118, 128) solid');
    await form(page).getByRole('button', { name: /記録して次へ/ }).focus();
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Tab');
    expect(await ring('.rf-record')).toBe('rgb(13, 118, 128) solid');
  });

  test('画面の文字に HubSpot の内部の値・開発者向けの言葉を出さない', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);
    const art = article(page);
    // 活動の状態は日本語 (fixture の通話は COMPLETED)。活動ログは中央の列のタブ
    await page.getByRole('tab', { name: '活動ログ' }).click();
    await expect(page.getByRole('region', { name: '活動ログ' }).locator('.wd-act-call .wd-act-meta').first()).toHaveText('発信 · 完了 · 通話時間 1分05秒');
    // tel: リンクは「端末の電話で発信」(href は tel:)
    const telLink = art.locator('.wd-phone-primary a[href^="tel:"]');
    await expect(telLink).toHaveText('端末の電話で発信');
    await expect(telLink).toHaveAttribute('href', 'tel:+81300000005');
    const visible = await page.evaluate(() => document.body.innerText);
    expect(visible).not.toMatch(/COMPLETED|NO_ANSWER|tel:|approved|Developer Docs|未確認|hubspot_|stage_labels|associations|通話ID/);
    expect(visible).not.toContain('記録済み(未送信)');
    // 架電結果の入力欄に内部名を出さない (パイプラインの名前「bpo_リクロジ」は HubSpot の表示名なので一覧の側にはある)
    expect(await page.locator('form[aria-label="架電結果の入力"]').evaluate((e) => e.textContent ?? '')).not.toMatch(/bpo_|risuto_|nitteigiri|saikadenn/);
  });
});
