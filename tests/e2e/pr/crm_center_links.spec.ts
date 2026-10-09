import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面 (/app/crm?mode=fixture) の「求人検索・リンク先」パネル: 案件のリンクを、新しいブラウザのタブではなくパネルの中に開く。
 *
 * - 架空サンプルの詳細 (workspaceFixture.ts) の「URL_求人検索」は Google 検索の URL (余計な項目付き)。
 *   枠には `igu=1` を付けて組み立て直した URL を出し、「新しいタブで開く」は元の URL のまま。
 * - 外のサイトには実際には繋がない: Google / example.com への要求は page.route で小さな HTML を返す。
 *   その HTML が枠の中に表示されることで、CSP の frame-src が https のページを許していることも確かめる。
 * - 「架電結果の入力」パネルに戻ったとき、入力途中のメモがそのまま残ることを確かめる (パネルは外さずに隠すだけ)。
 */

const SCREEN_URL = '/app/crm?mode=fixture';

async function stubExternal(page: Page): Promise<string[]> {
  const seen: string[] = [];
  await page.route(/^https:\/\/(www\.google\.com|www\.example\.com)\//, async (route) => {
    seen.push(route.request().url());
    await route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>stub</title><p id="stub">外部ページ(テスト用の代わり)</p>' });
  });
  return seen;
}

test.describe('CRM 架電画面: 求人検索・リンク先のパネルでリンクを開く', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('求人検索のリンクでパネルに Google 検索 (igu=1) の枠が開き、架電結果の入力に戻るとメモが残っている', async ({ page }) => {
    const external = await stubExternal(page);
    await page.goto(SCREEN_URL);
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
    const list = page.getByRole('list', { name: '架電キュー' });
    await list.locator('li.cq-row button.cq-row-button').first().click();

    const article = page.getByRole('article', { name: '架電先の詳細' });
    await expect(article.locator('h2')).toHaveText('ダミー建設');
    const dockTab = (name: string) => page.getByRole('tablist', { name: '中央の列のパネル' }).getByRole('tab', { name, exact: true });
    // 既定の配置 (v2): 「求人検索・リンク先」は右の列で前に出ている
    await expect(page.getByRole('tablist', { name: '右の列のパネル' }).getByRole('tab', { name: '求人検索・リンク先' })).toHaveAttribute('aria-selected', 'true');
    await dockTab('架電結果の入力').click();
    const form = page.getByRole('form', { name: '架電結果の入力' });
    const memo = form.locator('textarea').first();
    await memo.fill('受付で不在。来週火曜に再架電');

    // 選んだだけでは Google を読みに行かない
    expect(external).toEqual([]);
    const tabs = page.getByRole('tablist', { name: '求人検索・リンク先の表示' });
    await expect(tabs.getByRole('tab')).toHaveText(['リンク一覧', '求人検索']);
    await expect(tabs.getByRole('tab', { name: 'リンク一覧' })).toHaveAttribute('aria-selected', 'true');

    const searchLink = page.getByRole('region', { name: 'リンク' }).getByRole('link', { name: /求人を検索する/ });
    const stored = await searchLink.getAttribute('href');
    expect(stored).toContain('sca_esv=sample');
    await searchLink.click();

    // ブラウザの新しいタブは開かず、パネルの「求人検索」タブに枠が出る
    expect(page.context().pages()).toHaveLength(1);
    await expect(tabs.getByRole('tab', { name: '求人検索' })).toHaveAttribute('aria-selected', 'true');
    const panel = page.locator('#cq-cpanel-search');
    const frame = panel.locator('iframe[data-testid="link-frame"]');
    await expect(frame).toBeVisible();
    const src = await frame.getAttribute('src');
    const u = new URL(src ?? '');
    expect(u.origin + u.pathname).toBe('https://www.google.com/search');
    expect(u.searchParams.get('igu')).toBe('1');
    expect(u.searchParams.get('q')).toBe('03-0000-0005 求人');
    expect(u.searchParams.has('sca_esv')).toBe(false);
    await expect(frame).toHaveAttribute('sandbox', 'allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox');
    await expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer');
    // CSP が https の枠を許している (代わりの HTML が枠の中に表示される)
    await expect(page.frameLocator('#cq-cpanel-search iframe').locator('#stub')).toHaveText('外部ページ(テスト用の代わり)');
    await expect(panel.getByRole('link', { name: '新しいタブで開く' })).toHaveAttribute('href', stored ?? '');
    await expect(panel.getByText('表示されない場合は新しいタブで開いてください')).toBeVisible();
    // 右の列に開くので、案件の概要と架電結果の入力 (中央の列) は見えたまま。メモもそのまま
    await expect(article).toBeVisible();
    await expect(article.locator('h2')).toHaveText('ダミー建設');
    await expect(form).toBeVisible();
    await expect(memo).toHaveValue('受付で不在。来週火曜に再架電');
    await expect(frame).toBeVisible();

    // ホームページは別のタブに開き、×で閉じると求人検索のタブに戻る
    await tabs.getByRole('tab', { name: 'リンク一覧' }).click();
    await page.getByRole('region', { name: 'リンク' }).getByRole('link', { name: 'https://www.example.com/' }).click();
    await expect(tabs.getByRole('tab')).toHaveText(['リンク一覧', '求人検索', 'ホームページ']);
    await expect(page.frameLocator('iframe[src="https://www.example.com/"]').locator('#stub')).toBeVisible();
    await tabs.getByRole('button', { name: '「ホームページ」のタブを閉じる' }).click();
    await expect(tabs.getByRole('tab')).toHaveText(['リンク一覧', '求人検索']);
    await expect(tabs.getByRole('tab', { name: '求人検索' })).toHaveAttribute('aria-selected', 'true');
  });
});

test.describe('CRM 架電画面: 求人検索の枠の「戻る」と案内', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('枠の中で移動したあと「戻る」で枠だけが前のページに戻り、CRM 画面の URL は変わらない。Ctrl / ⌘ + クリックは新しいタブで開く', async ({ page }) => {
    await page.route(/^https:\/\/(www\.google\.com|www\.example\.com)\//, async (route) => {
      const u = new URL(route.request().url());
      const body = u.pathname === '/second'
        ? '<!doctype html><title>second</title><p id="second">2ページ目</p>'
        : '<!doctype html><title>first</title><p id="first">1ページ目</p><a id="next" href="/second">次へ</a> <a id="ext" href="https://www.example.com/ext">外部</a>';
      await route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body });
    });
    const openSearch = async () => {
      await page.goto(SCREEN_URL);
      await page.getByRole('list', { name: '架電キュー' }).locator('li.cq-row button.cq-row-button').first().click();
      await page.getByRole('region', { name: 'リンク' }).getByRole('link', { name: /求人を検索する/ }).click();
    };
    await openSearch();
    const panel = page.locator('#cq-cpanel-search');
    const inner = page.frameLocator('#cq-cpanel-search iframe');
    await expect(inner.locator('#first')).toBeVisible();
    const crmUrl = page.url();

    const hint = panel.getByTestId('link-hint');
    await expect(hint).toContainText('⌘ / Ctrl を押しながらクリック');

    const back = panel.getByRole('button', { name: '戻る' });
    await expect(back).toBeDisabled();
    await inner.locator('#next').click();
    await expect(inner.locator('#second')).toBeVisible();
    await expect(back).toBeEnabled();
    await back.click();
    await expect(inner.locator('#first')).toBeVisible();
    await expect(back).toBeDisabled();
    expect(page.url()).toBe(crmUrl);
    await expect(page.getByRole('article', { name: '架電先の詳細' })).toBeVisible();

    // Ctrl / ⌘ + クリックで新しいタブ (サンドボックスの枠の中のリンク)
    const popupPromise = page.context().waitForEvent('page', { timeout: 5000 }).catch(() => null);
    await inner.locator('#ext').click({ modifiers: [process.platform === 'darwin' ? 'Meta' : 'Control'] });
    const popup = await popupPromise;
    console.log(`CTRL_CLICK_POPUP=${popup !== null}`);
    expect(popup).not.toBeNull();

    await hint.getByRole('button', { name: '案内を閉じる' }).click();
    await expect(hint).toBeHidden();
    await openSearch();
    await expect(inner.locator('#first')).toBeVisible();
    await expect(panel.getByTestId('link-hint')).toHaveCount(0);
  });
});
