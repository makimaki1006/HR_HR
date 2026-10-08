import { expect, Locator, Page, Request, test } from '@playwright/test';
import { login } from './helpers/login';
import { E2E_EMAIL } from './helpers/fixture_values';

/**
 * 架電画面 (/app/crm、既定の表示 = 架電キュー) と架電結果の入力欄 (下書きのみ) の PR 用 E2E。
 *
 * - 架空サンプル (`?mode=fixture`) で動かす。画面は frontend/src/screens/crm/queueFixture.ts / workspaceFixture.ts /
 *   mocProperties.ts の固定値を使い、/api/crm/* を一切呼ばない (password ログインでは /api/crm/* は 403 になる)。
 *   ここでは「/api/crm/* へのリクエストが 0 件」「HubSpot への書き込み (GET 以外) が 0 件」も確かめる。
 * - この画面にグラフ (ECharts) は無い。値は一覧・詳細・入力欄に描画された文字と sessionStorage の中身で確かめる。
 * - 下書きと「記録済み」の印は sessionStorage (このタブだけ)。再読み込みでは残り、新しいタブには無い。
 */

const SCREEN_URL = '/app/crm?mode=fixture';
const STORAGE_KEY = 'hrhr.crm.callResult.v1';

/** JST の今日から n 日後 (YYYY-MM-DD) */
function jstDate(offsetDays: number): string {
  return new Date(Date.now() + 9 * 3600_000 + offsetDays * 86_400_000).toISOString().slice(0, 10);
}

function watchCrmRequests(page: Page): Request[] {
  const seen: Request[] = [];
  page.on('request', (r) => {
    const u = new URL(r.url());
    if (u.pathname.startsWith('/api/crm') || /hubapi\.com|hubspot\.com/.test(u.hostname)) seen.push(r);
  });
  return seen;
}

const list = (page: Page) => page.getByRole('list', { name: '架電キュー' });
const rows = (page: Page) => list(page).locator('li.cq-row');
const rowButton = (page: Page, i: number) => rows(page).nth(i).locator('button.cq-row-button');
const form = (page: Page) => page.getByRole('form', { name: '架電結果の入力' });
const outcome = (page: Page, label: string) => form(page).getByRole('group', { name: '今回の結果' }).getByRole('button', { name: label, exact: true });
const recordBtn = (page: Page) => form(page).getByRole('button', { name: /記録して次へ/ });
const fieldErrors = (f: Locator) => f.locator('.rf-err').allTextContents();

async function openScreen(page: Page): Promise<void> {
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await expect(list(page)).toBeVisible();
}

async function storedDrafts(page: Page): Promise<{ user?: string; drafts: Record<string, Record<string, unknown>>; recorded: Record<string, boolean> }> {
  return page.evaluate((k) => JSON.parse(window.sessionStorage.getItem(k) ?? '{"drafts":{},"recorded":{}}'), STORAGE_KEY);
}

test.describe('CRM 架電画面: 架電結果の下書き', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('一覧と詳細の値、結果ごとの必須欄、記録して次へ、再読み込み・新しいタブでの残り方。HubSpot には何も送らない', async ({ page, context }) => {
    const crmRequests = watchCrmRequests(page);
    await openScreen(page);

    // 一覧: 架空サンプルの 1 ページ目 (5 件)。先頭行の会社・担当者・番号・ステージ
    await expect(page.locator('.cq-count')).toContainText('全 10 件中 5 件を表示');
    await expect(rows(page)).toHaveCount(5);
    // 値は frontend/src/screens/crm/queueFixture.ts の SEEDS (既定の並び: f-5, f-2, f-11, f-7, f-3)
    await expect(rows(page).locator('.cq-row-company')).toHaveText(['ダミー建設', '架空食品株式会社', '架空ホテル', '見本不動産', 'サンプル運輸']);
    await expect(rows(page).nth(0).locator('.cq-row-contact')).toHaveText('丁野 三郎 ほか2人');
    await expect(rows(page).nth(0).locator('.cq-phone')).toHaveText('03-0000-0005');
    await expect(rows(page).nth(0).locator('.cq-stage')).toHaveText('担当者ブロック');
    await expect(rows(page).nth(4).locator('.cq-phone')).toHaveText('090-0000-0003');
    const firstCompany = 'ダミー建設';
    const secondCompany = '架空食品株式会社';

    // 行を選ぶ → 詳細の見出しに同じ会社名、入力欄が出る
    await rowButton(page, 0).click();
    await expect(rowButton(page, 0)).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText(firstCompany);
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('.wd-head .cq-stage')).toHaveText('担当者ブロック');
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('.wd-head-main p')).toHaveText('ダミー建設 採用支援');
    await expect(form(page).getByRole('status').filter({ hasText: '下書き' })).toHaveText('下書き(HubSpot 未送信)');
    await expect(form(page).locator('.rf-unsent')).toHaveText('この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません');
    // 結果の 6 択
    await expect(form(page).getByRole('group', { name: '今回の結果' }).getByRole('button'))
      .toHaveText(['担当者と会話', '不在・応答なし', '再架電の約束', 'アポイント獲得', '番号違い', '架電停止の希望']);

    // 再架電の約束: 次アクション「再架電」が自動で選ばれる。次回架電時間は HubSpot の定義どおり 45 択 + 未選択
    await outcome(page, '再架電の約束').click();
    await expect(form(page).getByRole('radio', { name: '再架電', exact: true })).toBeChecked();
    await expect(form(page).getByLabel(/^次回架電時間/).locator('option')).toHaveCount(46);

    // 押し間違えてアポに変えても、自動で入った「再架電」は外れ、必須はアポの 3 項目だけ
    await outcome(page, 'アポイント獲得').click();
    await expect(form(page).getByRole('radio', { name: '再架電', exact: true })).not.toBeChecked();
    await recordBtn(page).click({ force: true }); // aria-disabled のときも押せる (押すと足りない欄を示す)
    expect((await fieldErrors(form(page))).sort()).toEqual(['商談予定日を入れてください。', '商談予定時間を選んでください。', '商談方法を選んでください。'].sort());
    await expect(rowButton(page, 0)).toHaveAttribute('aria-pressed', 'true'); // 記録はしていない

    // 番号違い → 不通時チェックが必須。選択肢は HubSpot の表示ラベル
    await outcome(page, '番号違い').click();
    await recordBtn(page).click({ force: true }); // aria-disabled のときも押せる (押すと足りない欄を示す)
    expect(await fieldErrors(form(page))).toEqual(['不通時チェックを選んでください。']);
    await expect(form(page).getByRole('group', { name: /^不通時チェック/ }).or(form(page).getByLabel(/^不通時チェック/)).first()).toContainText('現在使われておりません');

    // 架電停止の希望 → 架電禁止理由が必須
    await outcome(page, '架電停止の希望').click();
    await recordBtn(page).click({ force: true }); // aria-disabled のときも押せる (押すと足りない欄を示す)
    expect(await fieldErrors(form(page))).toEqual(['架電禁止理由を入れてください。']);

    // 再架電の約束に戻し、日付と時間を入れて記録して次へ
    await outcome(page, '再架電の約束').click();
    await expect(form(page).getByRole('radio', { name: '再架電', exact: true })).toBeChecked();
    const nextDate = jstDate(1);
    await form(page).getByLabel(/^次回架電日/).fill(nextDate);
    await form(page).getByLabel(/^次回架電時間/).selectOption('9:15');
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'false');
    await recordBtn(page).click({ force: true }); // aria-disabled のときも押せる (押すと足りない欄を示す)

    // 先頭行に印、選択は 2 行目へ、入力欄は 2 行目の案件
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveText('記録済み(HubSpot 未送信)');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveAttribute('title', 'この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません');
    await expect(rowButton(page, 1)).toHaveAttribute('aria-pressed', 'true');
    await expect(rowButton(page, 0)).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText(secondCompany);
    await expect(rows(page).nth(1).locator('.cq-recorded')).toHaveCount(0);

    // sessionStorage の中身: 架空サンプルの鍵 (fixture:) だけ。値は選択肢の内部値・JST の暦日
    const saved = await storedDrafts(page);
    const keys = Object.keys(saved.drafts);
    expect(keys).toHaveLength(1);
    expect(keys[0]).toMatch(/^fixture:/);
    expect(saved.recorded).toEqual({ [keys[0] as string]: true });
    expect(saved.drafts[keys[0] as string]).toMatchObject({ outcome: 'callback', nextAction: '再架電', nextCallDate: nextDate, nextCallTime: '9:15', nextActionAuto: true });

    // 再読み込み (同じタブ): 印と下書きは残る
    await page.reload();
    await expect(list(page)).toBeVisible();
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveText('記録済み(HubSpot 未送信)');
    await rowButton(page, 0).click();
    await expect(outcome(page, '再架電の約束')).toHaveAttribute('aria-pressed', 'true');
    await expect(form(page).getByLabel(/^次回架電日/)).toHaveValue(nextDate);
    await expect(form(page).getByRole('status').filter({ hasText: '記録済み' })).toHaveText('記録済み(HubSpot 未送信)');

    // 記録の後に書き換えると印が外れ、その旨が出る
    await form(page).getByLabel(/^タスクメモ/).fill('追記');
    await expect(form(page).locator('.rf-notice')).toHaveText('内容を変えたので「記録済み」の印を外しました。もう一度「記録して次へ」を押してください。');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveCount(0);

    // 新しいタブ (同じブラウザ): sessionStorage はタブごとなので、印も下書きも無い (画面の説明どおり)
    const tab2 = await context.newPage();
    const crm2 = watchCrmRequests(tab2);
    await openScreen(tab2);
    await expect(rows(tab2)).toHaveCount(5);
    await expect(tab2.locator('.cq-recorded')).toHaveCount(0);
    await tab2.close();

    // HubSpot / CRM API へのリクエストは 0 件 (架空サンプルは接続しない。書き込みもしない)
    expect(crmRequests.map((r) => `${r.method()} ${r.url()}`)).toEqual([]);
    expect(crm2.map((r) => `${r.method()} ${r.url()}`)).toEqual([]);
  });

  test('sessionStorage に書けないときは赤で知らせ、「記録済み(HubSpot 未送信)」とは出さない。再読み込みで消える', async ({ page }) => {
    await page.addInitScript(() => {
      Storage.prototype.setItem = () => { throw new DOMException('blocked', 'QuotaExceededError'); };
    });
    await openScreen(page);
    await rowButton(page, 0).click();
    const alert = form(page).getByTestId('unsaved-alert');
    await expect(alert).toBeVisible();
    await expect(alert).toHaveAttribute('role', 'alert');
    await expect(alert).toContainText('この画面を閉じたり再読み込みしたりすると入力が消えます');
    await expect(form(page).locator('.rf-status')).toHaveText('下書き(この画面を閉じると消えます)');
    await expect(form(page).locator('.rf-status')).toHaveClass(/is-unsaved/);

    await outcome(page, '担当者と会話').click();
    await recordBtn(page).click({ force: true }); // aria-disabled のときも押せる (押すと足りない欄を示す)
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveText('記録済み(画面を閉じると消えます)');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveClass(/is-unsaved/);
    await expect(page.getByText('記録済み(HubSpot 未送信)')).toHaveCount(0);

    await page.reload();
    await expect(list(page)).toBeVisible();
    await expect(page.locator('.cq-recorded')).toHaveCount(0);
  });

  test('共用の PC: 同じタブでログアウトして別の人がログインすると、前の人の下書きのメモ・記録済みの印は出ず、タブからも消える', async ({ page }) => {
    const memoText = '受付の方に折り返しを依頼(E2E 前の人のメモ)';
    await openScreen(page);
    await rowButton(page, 0).click();
    await outcome(page, '担当者と会話').click();
    await form(page).locator('textarea').fill(memoText);
    await recordBtn(page).click({ force: true });
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveText('記録済み(HubSpot 未送信)');
    const before = await storedDrafts(page);
    expect(before.user).toBe(E2E_EMAIL);
    expect(Object.values(before.drafts).map((d) => d.memo)).toContain(memoText);

    // 同じタブでログアウト → 別の人がログイン → 架電画面
    await page.goto('/logout');
    await expect(page).toHaveURL(/\/login/);
    const other = 'e2e-other@f-a-c.co.jp';
    await login(page, other);
    await openScreen(page);
    await expect(rows(page)).toHaveCount(5);
    await expect(page.locator('.cq-recorded')).toHaveCount(0);
    await rowButton(page, 0).click();
    await expect(outcome(page, '担当者と会話')).toHaveAttribute('aria-pressed', 'false');
    await expect(form(page).locator('textarea')).toHaveValue('');
    await expect(page.getByText(memoText)).toHaveCount(0);
    // タブに残っていた前の人の分は、次の人の (空の) 下書きで置き換わっている
    const after = await storedDrafts(page);
    expect(after).toEqual({ user: other, drafts: {}, recorded: {} });
    expect(JSON.stringify(after)).not.toContain(memoText);
  });

  test('既定の架電画面では、見本・MOC の画面のファイルを読み込まない。?view=moc では見本の画面が出る', async ({ page }) => {
    const scripts: string[] = [];
    page.on('request', (r) => { if (r.resourceType() === 'script' || r.resourceType() === 'stylesheet') scripts.push(new URL(r.url()).pathname); });
    await openScreen(page);
    await rowButton(page, 0).click();
    await expect(form(page)).toBeVisible();
    expect(scripts.some((p) => /\/CallQueueScreen-[^/]+\.js$/.test(p))).toBe(true);
    expect(scripts.some((p) => /\/CallQueueScreen-[^/]+\.css$/.test(p))).toBe(true);
    expect(scripts.filter((p) => /\/(CrmScreen|CallWorkspace)-[^/]+\.(js|css)$/.test(p))).toEqual([]);
    // 架電画面の見た目 (CSS) が当たっている: 行のボタンは縦に並ぶ flex (queue.css)
    expect(await rowButton(page, 0).evaluate((b) => getComputedStyle(b).display)).toBe('flex');

    scripts.length = 0;
    await page.goto('/app/crm?view=moc');
    await expect(page.getByRole('banner').locator('strong')).toHaveText('連続架電ワークスペース');
    await expect(page.getByRole('navigation', { name: '画面表示' }).getByRole('link')).toHaveText(['1件ずつの表示', '基準のCRM画面']);
    await expect(page.getByText('12 / 12件 · 記録 0件')).toBeVisible();
    expect(scripts.some((p) => /\/CrmScreen-[^/]+\.js$/.test(p))).toBe(true);
    expect(scripts.filter((p) => /\/CallQueueScreen-[^/]+\.js$/.test(p))).toEqual([]);
  });
});
