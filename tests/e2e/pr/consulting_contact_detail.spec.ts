import { test, expect, Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { consultingFixture } from './helpers/consulting_layout_fixture';
import { PR_BASE_URL } from './helpers/fixture_values';

// 架空の API 応答だけで、案件の詳細の接触履歴を操作する（実データは使わない）。
const template = readFileSync(resolve(__dirname, '../../../templates/tabs/cs_dashboard.html'), 'utf8');
const other = (() => { const o = JSON.parse(readFileSync(resolve(__dirname, '../../fixtures/cs_dashboard/deal-detail-contact-ui.json'), 'utf8')); o.deal.deal_id = '90000000002'; o.deal.name = '架空の前契約'; o.events = o.events.slice(0, 2); o.counts.call = 2; return o; })();
const detail = JSON.parse(readFileSync(resolve(__dirname, '../../fixtures/cs_dashboard/deal-detail-contact-ui.json'), 'utf8'));

type Mode = { failApi?: boolean; goals?: 'ok' | 'fail' | 'partial' };
const state = { failApi: false };
async function open(page: Page, mode: boolean | Mode = {}, id = '90000000001') {
  const m: Mode = typeof mode === 'boolean' ? { failApi: mode } : mode;
  const failApi = !!m.failApi;
  state.failApi = failApi;
  const fixture = consultingFixture(1, 3, false);
  await page.route('**/consulting', r => r.fulfill({ contentType: 'text/html', body: template.replaceAll('{{ user }}', '架空の利用者') }));
  await page.route('**/api/crm/workspace/deals/**', r => {
    if (m.goals === 'fail') return r.fulfill({ status: 502, body: 'x' });
    if (m.goals === 'partial') return r.fulfill({ json: { selected: { deal: {} }, partial: [{ part: 'selected_properties' }] } });
    return r.fulfill({ json: { selected: { deal: { shokai_mtg_mokuhyou_risou: '架空の理想', shokai_mtg_mokuhyou_zenshin: '架空の前進' } }, partial: [] } });
  });
  await page.route('**/api/consulting/**', r => {
    const u = new URL(r.request().url());
    if (u.pathname.endsWith('/deal-detail')) return state.failApi ? r.fulfill({ status: 500, body: 'x' }) : r.fulfill({ json: u.searchParams.get('deal_id') === '90000000002' ? other : detail });
    return r.fulfill({ json: fixture.api(u.pathname, u.searchParams) });
  });
  await page.goto(`${PR_BASE_URL}/consulting#deal/detail?id=${id}`);
}

test('接触履歴: 件数・長文の全文・記録の詳細・フィルター・古い履歴', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  const tl = page.locator('#dd-tl');
  await expect(tl).toBeVisible();
  // 直近3か月の既定: 2026-08-02 以降の 9 件のうち、60 秒以下の電話を除いた 8 件。全 10 件。
  await expect(page.locator('#dd-count b')).toHaveText(/10 件中 7 件/);
  await expect(page.locator('#dd-older')).toContainText('1 件');
  // 60秒ちょうどは出ず、61秒は出る。
  await expect(tl).toContainText('1分1秒');
  await expect(tl).not.toContainText('1分0秒');
  await page.locator('#dd-c-all').click();
  await expect(tl.locator('li.k-call')).toHaveCount(2);
  // 長文は先頭だけ → 全文を表示（キーボード）
  const first = tl.locator('li.k-call').first();
  const more = first.locator('details.longtxt summary').first();
  await expect(more).toContainText('全文を表示');
  await more.focus();
  await page.keyboard.press('Enter');
  await expect(first.locator('details.longtxt[open] .full').first()).toContainText('架空の長文。'.repeat(40));
  // 記録の詳細（モデル名）
  await first.locator('details.evd summary').focus();
  await page.keyboard.press('Enter');
  await expect(first.locator('details.evd')).toContainText('架空モデル');
  await expect(first).toContainText('文字起こしあり');
  // 推定・期間外の印は閉じていても見える
  await expect(tl).toContainText('MTG（推定）');
  await expect(tl).toContainText('同じ日に録画');
  await expect(tl).toContainText('契約期間の外');
  await expect(tl).toContainText('未抽出');
  // 録画 MTG の詳細
  const mtg = tl.locator('li.k-mtg').filter({ hasText: '前向きシグナル' }).first();
  await expect(mtg.locator('details.evd')).toContainText('架空の前向き');
  // 応募報告は既定で出ず、チェックで出る
  await expect(tl).not.toContainText('架空の応募報告');
  await page.locator('#dd-mail-applications').check();
  await expect(tl).toContainText('架空の応募報告');
  await page.locator('#dd-k-email').uncheck();
  await expect(tl.locator('li.k-email')).toHaveCount(0);
  await page.locator('#dd-older').click();
  await expect(page.locator('#dd-recent')).toBeVisible();
  // 案件管理・HubSpot の導線、契約の連なり
  await expect(page.locator('.dd-open a[href="/app/job-copy?deal=90000000001"]')).toBeVisible();
  await expect(page.locator('#dd-chain')).toBeAttached();
  await expect(page.locator('#dd-goals')).toContainText('架空の理想');
  // 横スクロールが出ない
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/pc.png` : undefined, fullPage: true });
});

test('接触履歴: スマホ幅で横スクロールなし', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  await expect(page.locator('#dd-tl')).toBeVisible();
  await page.locator('#dd-c-all').click();
  await page.locator('#dd-older').click();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/sp.png` : undefined, fullPage: true });
});

test('接触履歴: 詳細APIが失敗したらエラー文と再試行ボタンを出し、再試行で復旧する', async ({ page }) => {
  await open(page, { failApi: true });
  const err = page.locator('#cs-error');
  await expect(err).toContainText('データを取得できませんでした');
  await expect(page.locator('#cs-status')).toContainText('を読み込めませんでした');
  await expect(page.locator('#dd-tl')).toHaveCount(0);
  await expect(page.locator('#cs-retry')).toHaveText('もう一度読み込む');
  state.failApi = false;
  await page.locator('#cs-retry').click();
  await expect(page.locator('#dd-tl')).toBeVisible();
  await expect(page.locator('#cs-error')).toBeHidden();
});

test('初回MTGの目標: 読めない・一部だけ読めないときは理由を出し、案件管理への導線を残す', async ({ page }) => {
  await open(page, { goals: 'fail' });
  await expect(page.locator('#dd-goals')).toContainText('HubSpot から読めませんでした');
  await expect(page.locator('#dd-goals a[href="/app/job-copy?deal=90000000001"]')).toBeVisible();
  await expect(page.locator('#dd-tl')).toBeVisible(); // 目標の失敗で履歴は消えない
  const p2 = await page.context().newPage();
  await open(p2, { goals: 'partial' });
  await expect(p2.locator('#dd-goals')).toContainText('目標の項目を読めませんでした');
});

test('案件切替と戻る: 別の案件を開いて戻ると元の案件の履歴に戻る', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  await expect(page.locator('#dd-goals')).toHaveAttribute('data-deal', '90000000001');
  await page.locator('#dd-chain summary').click();
  await page.locator('#dd-chain a.deallink').first().click();
  await expect(page.locator('#dd-goals')).toHaveAttribute('data-deal', '90000000002');
  await expect(page.locator('#dd-count b')).toContainText('件中');
  await page.goBack();
  await expect(page.locator('#dd-goals')).toHaveAttribute('data-deal', '90000000001');
  await expect(page.locator('#dd-count b')).toHaveText(/10 件中 7 件/);
});

test('次の対応: 長い本文は上部でも先頭だけ出て、全文を表示/閉じるが状態どおりに出る', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  const box = page.locator('.dd-nextbox');
  await expect(box.locator('h3')).toHaveText('次の対応と顧客の懸念');
  await expect(box.locator('.src')).toContainText('出典: 直近の電話（2026-10-30）の AI 要約。誤りを含むことがあります。');
  await expect(page.locator('.dd-nextbox').getByText('読み方')).toHaveCount(0);
  const d = box.locator('details.longtxt').first();
  await expect(d.locator('.t-close')).toBeVisible();
  await expect(d.locator('.t-open')).toBeHidden();
  await expect(d.locator('.pv')).toBeVisible();
  await d.locator('summary').focus();
  await page.keyboard.press('Enter');
  await expect(d.locator('.t-open')).toBeVisible();
  await expect(d.locator('.t-close')).toBeHidden();
  await expect(d.locator('.pv')).toBeHidden();
  await expect(d.locator('.full')).toContainText('架空の次の対応。'.repeat(30));
});

test('初回MTGの目標: 理想と前進ラインの両方を実装のプロパティ名どおりに表示する', async ({ page }) => {
  await open(page);
  await expect(page.locator('#dd-goals')).toContainText('架空の理想');
  await expect(page.locator('#dd-goals')).toContainText('架空の前進');
  await expect(page.locator('#dd-goals')).not.toContainText('未入力');
});

test('件数帯: 表示件数と絞り込みは常時、種別の内訳は「件数の内訳」で展開できる', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  const c = page.locator('#dd-count');
  await expect(c.locator('b')).toHaveText(/10 件中 7 件/);
  await expect(c).toContainText('直近3か月');
  const d = c.locator('details.cnt');
  await expect(d.locator('summary')).toHaveText('件数の内訳');
  await expect(d.locator('span.muted')).toBeHidden();
  await d.locator('summary').focus();
  await page.keyboard.press('Enter');
  await expect(d.locator('span.muted')).toContainText('抽出済み 1');
  await expect(d.locator('span.muted')).toContainText('契約期間の外');
  await expect(d.locator('span.muted')).toContainText('応募の報告 1 件');
});

test('新しい操作版の見た目: navyヘッダー・ゴシック・カード構成で、他の画面には漏れない', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  await expect(page.locator('#dd-tl')).toBeVisible();
  await expect(page.locator('body')).toHaveClass(/dd-page/);
  const css = await page.evaluate(() => {
    const cs = (sel: string) => getComputedStyle(document.querySelector(sel)!);
    return {
      headBg: cs('.masthead').backgroundColor,
      headFont: cs('.masthead h1').fontFamily,
      titleSize: cs('.dd-head h2.sec').fontSize,
      cardRadius: cs('.dd-kv > div').borderRadius,
      cardBorder: cs('.dd-nextbox').borderTopWidth,
      recHeadBg: cs('.tl .rec-head').backgroundColor,
      fieldCols: cs('.tl .what dl').gridTemplateColumns,
      rules: document.querySelectorAll('.rule-thin').length && getComputedStyle(document.querySelector('.rule-thin')!).display,
    };
  });
  expect(css.headBg).toBe('rgb(23, 47, 73)');
  expect(css.headFont.toLowerCase()).not.toMatch(/^"?(yu mincho|yumincho|hiragino mincho)/);
  expect(css.titleSize).toBe('28px');
  expect(css.cardRadius).toBe('8px');
  expect(css.cardBorder).toBe('1px');
  expect(css.recHeadBg).toBe('rgb(241, 245, 249)');
  expect(css.fieldCols.split(' ')[0]).toBe('140px');
  expect(css.rules).toBe('none');
  // 画面の順: 見出し → 状況カード → 次の対応 → 初回目標 → 接触履歴 → 契約の連なり
  const order = await page.evaluate(() => ['.dd-head', '.dd-kv', '.dd-nextbox', '#dd-goals', '#dd-hist', '#dd-chaincard']
    .map(s => document.querySelector(s)!.getBoundingClientRect().top));
  expect([...order].sort((a, b) => a - b)).toEqual(order);
  // 鮮度の警告は見出しの帯の中に残る
  await expect(page.locator('#cs-fresh')).toBeVisible();
  // 別の画面へ移ると、この見た目は外れる
  await page.locator('#cs-side button', { hasText: '今日' }).first().click();
  await expect(page.locator('body')).not.toHaveClass(/dd-page/);
  expect(await page.evaluate(() => getComputedStyle(document.querySelector('.masthead')!).backgroundColor)).not.toBe('rgb(23, 47, 73)');
});
