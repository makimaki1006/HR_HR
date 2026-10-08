import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { startWithResultPanelInFront } from './helpers/crm_layout';

/**
 * 架電画面 (/app/crm、架空サンプル `?mode=fixture`) で、入力を失わないための仕組みの PR 用 E2E。
 *
 * - 画面ファイルを読み込めなかったとき: 白い画面ではなく、案内と再読み込みのボタンが出る。押すと画面が出る
 * - 「下書きを消す」: 1 回押しただけでは消さず確かめる。やめる / Esc ではメモが残る。下書きが空なら押せない
 * - 「記録して次へ」: 読み込んだ行が全部記録済みでも続きのページがあれば、「さらに読み込む」を案内する
 * - 一覧を下端までスクロールすると続きのページを読み、全件を読んだら止まる (見出しは「全 N 件中 M 件を表示」)
 *
 * 架空サンプルは frontend/src/screens/crm/queueFixture.ts の固定値 (1 ページ 5 件、既定の条件で全 10 件)。/api/crm/* は呼ばない。
 * この画面にグラフ (ECharts) は無い。値は描画された文字と sessionStorage の中身で確かめる。
 */

const SCREEN_URL = '/app/crm?mode=fixture';
const STORAGE_KEY = 'hrhr.crm.callResult.v1';

const list = (page: Page) => page.getByRole('list', { name: '架電キュー' });
const rows = (page: Page) => list(page).locator('li.cq-row');
const rowButton = (page: Page, i: number) => rows(page).nth(i).locator('button.cq-row-button');
const form = (page: Page) => page.getByRole('form', { name: '架電結果の入力' });
const outcome = (page: Page, label: string) => form(page).getByRole('group', { name: '今回の結果' }).getByRole('button', { name: label, exact: true });
const recordBtn = (page: Page) => form(page).getByRole('button', { name: /記録して次へ/ });
const memo = (page: Page) => form(page).getByLabel(/^タスクメモ/);

async function openScreen(page: Page): Promise<void> {
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await expect(list(page)).toBeVisible();
}

async function storedDrafts(page: Page): Promise<{ drafts: Record<string, Record<string, unknown>>; recorded: Record<string, boolean> }> {
  return page.evaluate((k) => JSON.parse(window.sessionStorage.getItem(k) ?? '{"drafts":{},"recorded":{}}'), STORAGE_KEY);
}

test.describe('CRM 架電画面: 入力を失わないための仕組み', () => {
  // 架電結果の入力欄を確かめるので、「架電結果の入力」を前に出した配置で始める (既定は「活動ログ」が前)
  test.beforeEach(async ({ page }) => { await startWithResultPanelInFront(page); await login(page); });

  test('画面ファイルを読み込めないときは白い画面にせず案内と再読み込みを出し、読み込めるようになれば再読み込みで画面が出る', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => { errors.push(e.message); });
    const block = /\/CallQueueScreen-[^/]+\.js$/;
    await page.route(block, (r) => r.abort());
    await page.goto(SCREEN_URL);
    const alert = page.getByRole('alert').filter({ hasText: '画面を読み込めませんでした' });
    await expect(alert).toHaveText(/画面を読み込めませんでした。通信の状態を確かめて、再読み込みしてください。/);
    await expect(alert).not.toContainText('.js');
    await expect(page.locator('#app-root')).not.toHaveText('');
    await expect(list(page)).toHaveCount(0);

    await page.unroute(block);
    await alert.getByRole('button', { name: '再読み込み' }).click();
    await expect(list(page)).toBeVisible();
    await expect(rows(page)).toHaveCount(5);
    await expect(rows(page).locator('.cq-row-company').first()).toHaveText('ダミー建設');
    await expect(page.getByRole('alert').filter({ hasText: '画面を読み込めませんでした' })).toHaveCount(0);
  });

  test('下書きを消す: 確かめてから消す。やめる / Esc ではメモが残り、空の下書きでは押せない', async ({ page }) => {
    await openScreen(page);
    await rowButton(page, 0).click();
    const clearBtn = form(page).getByRole('button', { name: '下書きを消す' });
    await expect(clearBtn).toBeDisabled();

    await outcome(page, '担当者と会話').click();
    await memo(page).fill('求人票を送る');
    await expect(clearBtn).toBeEnabled();

    await clearBtn.click();
    const ask = form(page).getByRole('group', { name: '下書きを消すか確認' });
    await expect(ask).toContainText('入力した内容(メモを含む)と「記録済み」の印を消します。元に戻せません。');
    // 隣の「記録して次へ」と押し間違えないよう、フォーカスは「やめる」
    await expect(ask.getByRole('button', { name: 'やめる' })).toBeFocused();
    await ask.getByRole('button', { name: 'やめる' }).click();
    await expect(memo(page)).toHaveValue('求人票を送る');
    await expect(clearBtn).toBeFocused();

    await clearBtn.click();
    await page.keyboard.press('Escape');
    await expect(ask).toHaveCount(0);
    await expect(memo(page)).toHaveValue('求人票を送る');
    expect(Object.values((await storedDrafts(page)).drafts).map((d) => d.memo)).toEqual(['求人票を送る']);

    await clearBtn.click();
    await ask.getByRole('button', { name: '消す', exact: true }).click();
    await expect(memo(page)).toHaveValue('');
    await expect(outcome(page, '担当者と会話')).toHaveAttribute('aria-pressed', 'false');
    await expect(clearBtn).toBeDisabled();
    expect(await storedDrafts(page)).toMatchObject({ drafts: {}, recorded: {} });
  });

  test('読み込んだ 5 件を全部記録しても続きがあれば「さらに読み込む」を案内し、押すと続きの未記録の行が出る', async ({ page }) => {
    await openScreen(page);
    await rowButton(page, 0).click();
    for (let i = 0; i < 5; i += 1) {
      await expect(rowButton(page, i)).toHaveAttribute('aria-pressed', 'true');
      await outcome(page, '不在・応答なし').click();
      await recordBtn(page).click();
    }
    await expect(rows(page).locator('.cq-recorded')).toHaveCount(5);
    await expect(form(page).locator('.rf-notice'))
      .toHaveText('表示中の一覧に未記録の架電先はありません。一覧の下の「さらに読み込む」で続きを表示できます。');
    await page.getByRole('button', { name: 'さらに読み込む' }).click();
    await expect(rows(page)).toHaveCount(10);
    await expect(rows(page).nth(5).locator('.cq-recorded')).toHaveCount(0);
  });
  test('一覧を下端までスクロールすると続きを読み込み、全件を読んだら止まる', async ({ page }) => {
    // 5 行で一覧の枠がスクロールするよう、画面の高さを低くする
    await page.setViewportSize({ width: 1280, height: 480 });
    await openScreen(page);
    await expect(rows(page)).toHaveCount(5);
    await expect(page.locator('.cq-count')).toContainText('全 10 件中 5 件を表示');
    const scroller = page.locator('.cq-list-scroll');
    expect(await scroller.evaluate((el) => el.scrollHeight > el.clientHeight)).toBe(true);
    // 開いただけでは読まない
    await expect(rows(page)).toHaveCount(5);
    await scroller.evaluate((el) => { el.scrollTop = el.scrollHeight; });
    await expect(rows(page)).toHaveCount(10);
    await expect(page.locator('.cq-count')).toContainText('全 10 件中 10 件を表示');
    await expect(page.getByText('これで最後です。')).toBeVisible();
    await expect(page.getByTestId('queue-load-sentinel')).toHaveCount(0);
    // 同じ行を 2 回出さない (行の文字 = 会社・担当者・番号 が 10 通り)
    const texts = await rows(page).locator('button.cq-row-button').allTextContents();
    expect(new Set(texts).size).toBe(10);
  });
});
