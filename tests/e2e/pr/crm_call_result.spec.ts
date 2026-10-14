import { expect, Page, Request, test } from '@playwright/test';
import { login } from './helpers/login';
import { startWithResultPanelInFront } from './helpers/crm_layout';
import { E2E_EMAIL } from './helpers/fixture_values';

/**
 * 架電画面 (/app/crm、既定の表示 = 架電キュー) と架電結果の入力欄 (HubSpot に実在する取引プロパティへ保存) の PR 用 E2E。
 *
 * - 架空サンプル (`?mode=fixture`) で動かす。画面は frontend/src/screens/crm/queueFixture.ts / workspaceFixture.ts /
 *   fakeWrite.ts の固定値を使い、/api/crm/* を一切呼ばない (保存は画面の中の架空の書き込み。HubSpot には何も送らない)。
 *   ここでは「/api/crm/* へのリクエストが 0 件」「HubSpot への書き込み (GET 以外) が 0 件」も確かめる。
 * - この画面にグラフ (ECharts) は無い。値は一覧・詳細・入力欄に描画された文字と sessionStorage の中身で確かめる。
 * - 入力途中の内容 (送る前の下書き) と「記録済み」の印は sessionStorage (このタブだけ)。再読み込みでは残り、新しいタブには無い。
 */

const SCREEN_URL = '/app/crm?mode=fixture';
const STORAGE_KEY = 'hrhr.crm.callResult.v2';

/** JST の今日から n 日後 (YYYY-MM-DD) */
function jstDate(offsetDays: number): string {
  return new Date(Date.now() + 9 * 3600_000 + offsetDays * 86_400_000).toISOString().slice(0, 10);
}
/** JST の今日の MM/DD (追記の行の頭) */
const todayPrefix = () => { const d = jstDate(0); return `${d.slice(5, 7)}/${d.slice(8, 10)}`; };

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
const stageBtn = (page: Page, label: string) => form(page).getByRole('group', { name: 'ステージ(よく使うもの)' }).getByRole('button', { name: label, exact: true });
const recordBtn = (page: Page) => form(page).getByRole('button', { name: /記録して次へ/ });
const field = (page: Page, label: string) => form(page).getByLabel(label, { exact: true });
const memo = (page: Page, label = 'タスクメモに追記') => form(page).getByRole('textbox', { name: label });

async function openScreen(page: Page, url = SCREEN_URL): Promise<void> {
  await page.goto(url);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await expect(list(page)).toBeVisible();
}

async function storedDrafts(page: Page): Promise<{ user?: string; drafts: Record<string, { stageId: string; values: Record<string, string> }>; recorded: Record<string, boolean> }> {
  return page.evaluate((k) => JSON.parse(window.sessionStorage.getItem(k) ?? '{"drafts":{},"recorded":{}}'), STORAGE_KEY);
}

test.describe('CRM 架電画面: 架電結果を実在の取引プロパティへ保存', () => {
  // 架電結果の入力欄を確かめるので、「架電結果の入力」を前に出した配置で始める (既定は「活動ログ」が前)
  test.beforeEach(async ({ page }) => { await startWithResultPanelInFront(page); await login(page); });

  test('BPO の組: 一覧と詳細の値、架電日・メモの初期表示、項目とステージを保存して次へ、再読み込み・新しいタブでの残り方。HubSpot には何も送らない', async ({ page, context }) => {
    const crmRequests = watchCrmRequests(page);
    // 案件の概要の詳しい表示 (ステージ・案件名) を確かめるので、背の高い画面で開く (800px 以下では 1 行の表示)
    await page.setViewportSize({ width: 1440, height: 900 });
    await openScreen(page);

    // 一覧: 目印が最初から見えているので、2 ページ目も自動で読み足され全 10 件になる。先頭行の会社・担当者・番号・ステージ
    await expect(page.locator('.cq-count')).toContainText('全 10 件中 10 件を表示');
    await expect(rows(page)).toHaveCount(10);
    // 値は frontend/src/screens/crm/queueFixture.ts の SEEDS (既定の並び: f-5, f-2, f-11, f-7, f-3 が先頭 5 件)
    const firstFive = ['ダミー建設', '架空食品株式会社', '架空ホテル', '見本不動産', 'サンプル運輸'];
    for (const [i, name] of firstFive.entries()) await expect(rows(page).nth(i).locator('.cq-row-company')).toHaveText(name);
    await expect(rows(page).nth(0).locator('.cq-row-contact')).toHaveText('丁野 三郎 ほか2人');
    await expect(rows(page).nth(0).locator('.cq-phone')).toHaveText('03-0000-0005');
    await expect(rows(page).nth(0).locator('.cq-stage')).toHaveText('担当者ブロック');

    // 行を選ぶ → 詳細の見出しに同じ会社名、入力欄は BPO の組 (チームが無いので、bpo_リクロジ のパイプラインで決まる)
    await rowButton(page, 0).click();
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText('ダミー建設');
    await expect(memo(page)).toBeVisible();
    await expect(form(page)).toHaveAttribute('data-team', 'bpo');
    await expect(form(page).getByTestId('result-group')).toHaveText('BPOの項目');
    await expect(form(page).locator('[data-field] .rf-label')).toHaveText(['架電日', '次回架電日', '次回架電時間', 'タスクメモ', '接触', '日程切り']);
    // ステージ: 今は「担当者ブロック」(押せない)。「要入力」は移すときに入力する項目があるステージ
    await expect(form(page).locator('.rf-stage-head')).toContainText('今: 担当者ブロック');
    await expect(form(page).getByRole('group', { name: 'ステージ(よく使うもの)' }).getByRole('button')).toHaveText(['不在', '受付ブロック', '不通要入力', '担当者ブロック', '日程確保', 'アポ日確定要入力']);
    await expect(stageBtn(page, '担当者ブロック')).toBeDisabled();
    // 架電日は前回の日 (2026-09-25) が入っていても今日。前回の日は欄の横に小さく出す。次回架電日・時間は今の値。次回架電時間は HubSpot の定義どおり 45 択 + 未入力
    await expect(field(page, '架電日')).toHaveValue(jstDate(0).replaceAll('-', '/'));
    await expect(form(page).getByTestId('call-date-prev')).toHaveText('前回: 09/25');
    await expect(field(page, '次回架電日')).toHaveValue('2026/09/30');
    await expect(field(page, '次回架電時間').locator('option')).toHaveCount(46);
    // メモは今の内容を読み取りで出し、下の欄は追記
    await expect(form(page).getByTestId('memo-current')).toHaveText('09/28 受付で不在。採用担当は火曜午後(架空)\n10/02 再度不在。折り返し依頼済み');
    await expect(memo(page)).toHaveValue('');
    // 画面に内部名を出さない
    expect(await form(page).innerText()).not.toMatch(/bpo_|risuto_|nitteigiri/);

    // ステージ「受付ブロック」+ 次回架電日・時間 + メモの追記。入力途中はタブに残る (送る前の下書き)
    const nextDate = jstDate(1);
    await stageBtn(page, '受付ブロック').click();
    await expect(stageBtn(page, '受付ブロック')).toHaveAttribute('aria-pressed', 'true');
    await field(page, '次回架電日').fill(nextDate.replaceAll('-', '/'));
    await field(page, '次回架電時間').selectOption('9:15');
    await memo(page).fill('受付で不在。来週再架電(E2E)');
    const draft = await storedDrafts(page);
    expect(draft.user).toBe(E2E_EMAIL);
    expect(draft.drafts['fixture:f-5']).toEqual({ stageId: '1095387444', values: { bpo_13: nextDate, bpo_14: '9:15', bpo_16: '受付で不在。来週再架電(E2E)' } });

    await recordBtn(page).click();
    // 先頭行に印、選択は 2 行目へ。次の入力欄に前の行を保存したことを緑で出す
    await expect(rows(page).nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み)');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveAttribute('title', 'この画面から架電結果を HubSpot に保存しました(この印はこの画面のタブだけに残ります)');
    await expect(rowButton(page, 1)).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText('架空食品株式会社');
    await expect(form(page).getByTestId('result-notice')).toHaveText('前の架電先: ダミー建設 を HubSpot に保存しました。');
    await expect(form(page).getByTestId('result-notice')).toHaveClass(/is-saved/);
    // 保存した下書きは消え、印だけ残る
    expect(await storedDrafts(page)).toMatchObject({ drafts: {}, recorded: { 'fixture:f-5': true } });

    // 先頭行に戻ると、保存した値が出ている (ステージ・次回架電日・時間・追記したメモ)
    await rowButton(page, 0).click();
    await expect(form(page).locator('.rf-stage-head')).toContainText('今: 受付ブロック');
    await expect(field(page, '次回架電日')).toHaveValue(nextDate.replaceAll('-', '/'));
    await expect(field(page, '次回架電時間')).toHaveValue('9:15');
    await expect(form(page).getByTestId('memo-current')).toHaveText(`09/28 受付で不在。採用担当は火曜午後(架空)\n10/02 再度不在。折り返し依頼済み\n${todayPrefix()} 受付で不在。来週再架電(E2E)`);

    // 再読み込み (同じタブ): 印は残る
    await page.reload();
    await expect(list(page)).toBeVisible();
    await expect(rows(page).nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み)');

    // 新しいタブ (同じブラウザ): sessionStorage はタブごとなので、印は無い
    const tab2 = await context.newPage();
    const crm2 = watchCrmRequests(tab2);
    await openScreen(tab2);
    await expect(rows(tab2)).toHaveCount(10);
    await expect(tab2.locator('.cq-recorded')).toHaveCount(0);
    await tab2.close();

    // HubSpot / CRM API へのリクエストは 0 件 (架空サンプルは接続しない)
    expect(crmRequests.map((r) => `${r.method()} ${r.url()}`)).toEqual([]);
    expect(crm2.map((r) => `${r.method()} ${r.url()}`)).toEqual([]);
  });

  test('必須のあるステージ (不通): 移す画面で必須を確かめ、入力欄の変更と一緒に保存する。やめると何も保存しない', async ({ page }) => {
    await page.setViewportSize({ width: 1512, height: 742 });
    await openScreen(page);
    await rowButton(page, 0).click();
    await expect(memo(page)).toBeVisible();
    await stageBtn(page, '不通').click();
    await expect(form(page).getByTestId('stage-needs-input')).toHaveText('「不通」に移すには必要な項目があります。記録するときに確かめます。');
    await memo(page).fill('番号が通じない(E2E)');
    await recordBtn(page).click();
    const modal = page.getByTestId('stage-modal');
    await expect(modal).toBeVisible();
    await expect(modal.getByRole('heading')).toHaveText('このステージに移すには次の項目が必要です');
    await expect(modal.getByTestId('stage-remaining')).toContainText('必須 あと 1 項目');
    // やめる: 何も保存せず入力欄に戻る (入力はそのまま)
    await modal.getByRole('button', { name: 'やめる' }).click();
    await expect(modal).toHaveCount(0);
    await expect(rowButton(page, 0)).toHaveAttribute('aria-pressed', 'true');
    await expect(memo(page)).toHaveValue('番号が通じない(E2E)');
    expect((await storedDrafts(page)).recorded).toEqual({});

    await recordBtn(page).click();
    await modal.getByLabel('不通時チェック').selectOption({ index: 1 });
    await expect(modal.getByTestId('stage-remaining')).toHaveText('必須項目はすべて入力済みです');
    await modal.getByRole('button', { name: '移す' }).click();
    await expect(modal).toHaveCount(0);
    await expect(rowButton(page, 1)).toHaveAttribute('aria-pressed', 'true');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み)');
    await rowButton(page, 0).click();
    await expect(form(page).locator('.rf-stage-head')).toContainText('今: 不通');
    await expect(form(page).getByTestId('memo-current')).toContainText(`${todayPrefix()} 番号が通じない(E2E)`);
  });

  test('新規営業の組 (BPO 以外のパイプライン): 再架電日・再架電時間・激アツメモ。TEL_アポ日確定ではアポ取得者に本人が入る', async ({ page }) => {
    await page.setViewportSize({ width: 1512, height: 742 });
    await openScreen(page, `${SCREEN_URL}&pipeline=fx-sample`);
    await list(page).getByText('架空倉庫サービス').click();
    await expect(memo(page, '激アツメモ（決済・利用サービス・決算・職種など）に追記')).toBeVisible();
    await expect(form(page)).toHaveAttribute('data-team', 'sales');
    await expect(form(page).getByTestId('result-group')).toHaveText('新規営業の項目');
    await expect(form(page).locator('[data-field] .rf-label')).toHaveText(['架電日', '再架電日', '再架電時間', '激アツメモ（決済・利用サービス・決算・職種など）']);
    // 架電日が空なので今日が入っている。激アツメモは 1 行の項目なので追記の欄も 1 行
    await expect(field(page, '架電日')).toHaveValue(jstDate(0).replaceAll('-', '/'));
    await expect(form(page).locator('.rf-memo-add input[type="text"]')).toHaveCount(1);

    await stageBtn(page, 'TEL_アポ日確定').click();
    await memo(page, '激アツメモ（決済・利用サービス・決算・職種など）に追記').fill('社長が決裁(E2E)');
    await recordBtn(page).click();
    const modal = page.getByTestId('stage-modal');
    await expect(modal).toBeVisible();
    // アポ取得者は本人 (架空 一郎)、架電日は入力欄の値 (今日)
    await expect(modal.getByRole('combobox', { name: 'アポ取得者' })).toHaveValue('架空 一郎');
    await expect(modal.getByLabel('架電日', { exact: true })).toHaveValue(jstDate(0).replaceAll('-', '/'));
    await modal.getByLabel('アポ取得日', { exact: true }).fill(jstDate(0).replaceAll('-', '/'));
    await modal.getByRole('button', { name: '移す' }).click();
    await expect(modal).toHaveCount(0);
    await list(page).getByText('架空倉庫サービス').click();
    await expect(form(page).locator('.rf-stage-head')).toContainText('今: TEL_アポ日確定');
    await expect(form(page).getByTestId('memo-current')).toHaveText(`決裁は本社。拠点は採用人数だけ決める(架空) / 来月から媒体の契約見直し / ${todayPrefix()} 社長が決裁(E2E)`);
  });

  test('sessionStorage に書けないときは知らせる。保存はできるが、記録済みの印は再読み込みで消える', async ({ page }) => {
    await page.addInitScript(() => {
      Storage.prototype.setItem = () => { throw new DOMException('blocked', 'QuotaExceededError'); };
    });
    await openScreen(page);
    await rowButton(page, 0).click();
    const alert = form(page).getByTestId('unsaved-alert');
    await expect(alert).toBeVisible();
    await expect(alert).toContainText('入力途中の内容をこの画面に残せません。保存する前に閉じたり再読み込みしたりすると、入力が消えます。');

    await memo(page).fill('残せない環境(E2E)');
    await recordBtn(page).click();
    await expect(rows(page).nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み。印は画面を閉じると消えます)');
    await expect(rows(page).nth(0).locator('.cq-recorded')).toHaveClass(/is-unsaved/);

    await page.reload();
    await expect(list(page)).toBeVisible();
    await expect(page.locator('.cq-recorded')).toHaveCount(0);
  });

  test('共用の PC: 同じタブでログアウトして別の人がログインすると、前の人の入力途中のメモは出ず、タブからも消える', async ({ page }) => {
    const memoText = '受付の方に折り返しを依頼(E2E 前の人のメモ)';
    await openScreen(page);
    await rowButton(page, 0).click();
    await memo(page).fill(memoText);
    const before = await storedDrafts(page);
    expect(before.user).toBe(E2E_EMAIL);
    expect(Object.values(before.drafts).map((d) => d.values.bpo_16)).toContain(memoText);

    // 同じタブでログアウト → 別の人がログイン → 架電画面
    await page.goto('/logout');
    await expect(page).toHaveURL(/\/login/);
    const other = 'e2e-other@f-a-c.co.jp';
    await login(page, other);
    await openScreen(page);
    await expect(rows(page)).toHaveCount(10);
    await rowButton(page, 0).click();
    await expect(memo(page)).toHaveValue('');
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
