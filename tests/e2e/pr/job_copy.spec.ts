import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 求人文面管理 (/app/job-copy) を Rust が配る React 画面で確かめる。
 * 求人と応募の取得 (/api/job-copy/moc) は Drive の保存データを読むので、PR の fixture には無い。
 * ここでは合成データを page.route で返し、市場 (/api/job-copy/market) も合成の月次データを返す。
 * 確かめること: 最初のタブがタイムライン、7 レーンの値、2 つの ECharts の描画完了、期間比較表の具体値、
 * 課金CSV を読み込むと課金レーンと期間比較表に入ること、取込と逆検索が主作業の外にあること。
 */
const defaultCapturedAt = '2026-08-20T00:00:00Z';
const months = Array.from({ length: 14 }, (_, index) => `${2025 + Math.floor((index + 6) / 12)}-${String((index + 6) % 12 + 1).padStart(2, '0')}`);

function snapshot(capturedAt = defaultCapturedAt) {
  return {
    schemaVersion: 1, capturedAt,
    capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{
      id: 'synthetic-pr-timeline', hubspotListingId: '30', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678',
      location: '大分県大分市', body: '仕事内容：合成の配送業務です。\n給与：月給250,000円〜280,000円\n休日：土日', images: [],
      history: [{ id: 'synthetic-pr-timeline-previous', capturedAt: '2026-07-01T00:00:00Z', body: '仕事内容：合成の配送業務です。\n給与：月給230,000円〜260,000円\n休日：土日', images: [] }],
    }] },
    results: [{ listing_id: '30', summary: { total: 6, missing_date: 1, by_date: { '2026-07-10': 2, '2026-07-25': 1, '2026-08-20': 2 }, dimensions: { gender: { 男性: 4, 不明: 2 } } }, dated_comparison: null }],
  };
}

async function open(page: Page, capturedAt = defaultCapturedAt, options: { seriesFailures?: number } = {}) {
  const calls: string[] = [];
  let seriesFailures = options.seriesFailures ?? 0;
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(snapshot(capturedAt)) }));
  await page.route('**/api/job-copy/market*', route => {
    const url = new URL(route.request().url());
    calls.push(url.search);
    if (url.searchParams.has('title') && seriesFailures > 0) {
      seriesFailures -= 1;
      return route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ error: 'synthetic failure' }) });
    }
    const selected = url.searchParams.get('title') === '配送ドライバー' && url.searchParams.get('prefecture') === '大分県';
    return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
      source: '合成の市場データ', titles: ['配送ドライバー', '倉庫作業'], prefectures: ['大分県', '福岡県'], ctk_basis: '合成の閲覧者指標です。応募数ではありません。',
      series: selected ? { prefecture: '大分県', months, job_count: months.map((_, index) => 100 + index * 10), ctk_count: months.map((_, index) => 600 + index), employer_count: months.map(() => 30), seekers_per_posting: months.map(() => 5) } : null,
    }) });
  });
  await login(page);
  await page.goto('/app/job-copy');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  return calls;
}

async function seriesLengths(page: Page, testId: string) {
  const chart = page.locator(`[data-testid="${testId}"][data-chart-ready="true"]`);
  await expect(chart).toHaveCount(1);
  return chart.evaluate(element => {
    const option = window.__echarts_getInstanceByDom?.(element as HTMLElement)?.getOption() as { series?: { data?: unknown[] }[] } | undefined;
    return option?.series?.map(series => series.data?.length ?? 0) ?? [];
  });
}

test.describe('求人文面管理のタイムライン', () => {
  test('最初のタブがタイムラインで、7 レーン・グラフ・期間比較表に具体値が出る', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    const calls = await open(page);
    const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
    await expect(primary.getByRole('tab')).toHaveText(['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']);
    await expect(primary.getByRole('tab', { name: 'タイムライン', exact: true })).toHaveAttribute('aria-selected', 'true');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    for (const lane of ['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場']) await expect(timeline.getByRole('group', { name: lane, exact: true })).toBeVisible();
    const salary = timeline.getByRole('group', { name: '給与', exact: true });
    await expect(salary).toContainText('月給23万〜26万円');
    await expect(salary).toContainText('月給25万〜28万円');
    // HRハッカー実績も課金CSVも無いので、課金は「課金データなし」で 0円 の金額にしない
    await expect(timeline.getByRole('group', { name: '課金', exact: true })).toHaveText(/課金データなし（0円という意味ではありません）/u);
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing')).toHaveCount(0);
    await expect(timeline).toContainText('応募日が分からない応募 1件 はグラフに含めていません');
    // 市場は求人タイトルと勤務地から自動で選び、選んだ値を見せる
    await expect(timeline.getByLabel('職種')).toHaveValue('配送ドライバー');
    await expect(timeline.getByLabel('都道府県')).toHaveValue('大分県');
    expect(calls.some(search => search.includes('title=%E9%85%8D%E9%80%81%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC'))).toBe(true);
    const applications = await seriesLengths(page, 'jt-applications');
    expect(applications.length).toBeGreaterThan(0);
    expect(applications[0]).toBeGreaterThan(0);
    const market = await seriesLengths(page, 'jt-market');
    expect(market.length).toBe(2);
    expect(market[0]).toBeGreaterThan(0);
    // 期間比較表: 取得日 07-01 と 08-20 で区切った 2 期間（応募は HubSpot 記録分）
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0).locator('td').nth(0)).toHaveText('50日');
    await expect(rows.nth(0).locator('td').nth(1)).toHaveText('3件');
    await expect(rows.nth(0).locator('td').nth(2)).toHaveText('0.06件/日');
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('課金データなし');
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('+4.5%（2026/07 220件 → 2026/08 230件）');
    await expect(rows.nth(1).locator('td')).toHaveText(['1日', '2件', '2.00件/日', '課金データなし', '同じ月の中（2026/08 230件）']);
    // 版の名前は日付で書く（「過去CSVの版」「媒体CSV取得版」は出さない）
    await expect(rows.nth(0).locator('th')).toContainText('2026/07/01時点の求人内容');
    await expect(rows.nth(1).locator('th')).toContainText('2026/08/20時点の求人内容');
    await expect(page.locator('body')).not.toContainText(/過去CSVの版|媒体CSV取得版|接続待ち|実求人ID|原本不足/u);
    // 上の帯の件数は期間比較表と同じ割り当てで数える（応募日なし 1 件 + 期間外 0 件 = 1 件。全 6 件ではない）
    await page.locator('.jc-snapshot-tip > summary').click();
    await expect(page.getByRole('region', { name: '実データの取得範囲' })).toContainText('掲載期間に入らない応募 1件');
    await page.locator('.jc-snapshot-tip > summary').click();
    // 市場の選び方: 職種と都道府県を別々に、自動で選んだことを示す
    await expect(timeline.getByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください', { exact: true })).toBeVisible();
    await expect(timeline.getByText('勤務地から大分県を自動で選びました', { exact: true })).toBeVisible();
    // 主作業の外: データ取込は閉じていて、逆検索は押すまで出ない。旧来の操作バーも無い。
    await expect(page.locator('.jc-data-import')).toBeHidden();
    await expect(page.locator('.jc-reverse-search')).toHaveCount(0);
    await expect(page.getByRole('button', { name: '求人一覧に戻る', exact: true })).toHaveCount(0);
    const firstView = await page.evaluate(() => ({ inner: window.innerHeight, list: document.querySelector('.jc-list')?.getBoundingClientRect().bottom ?? 0 }));
    expect(firstView.list).toBeGreaterThanOrEqual(firstView.inner - 2);
    await expect(page.locator('body')).not.toContainText(/効果|確実に|必ず|100%/u);
  });

  test('最新の版の給与・本文・画像の印が右端で切れずに読める', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    // 最新の版は取得日 (08-20) に始まり、時間軸の右端 (約 99%) に来る。
    const checks: [string, string, string][] = [
      ['給与', '.jt-salary-label', '▲月給25万〜28万円'],
      ['本文', '.jt-mark', '追加1・削除1'],
      ['画像', '.jt-mark', '不明'],
    ];
    for (const [lane, selector, text] of checks) {
      const group = timeline.getByRole('group', { name: lane, exact: true });
      const marker = group.locator(selector).last();
      await expect(marker).toHaveText(text);
      const box = await marker.evaluate(element => {
        const track = element.closest('.jt-track')?.getBoundingClientRect();
        const own = element.getBoundingClientRect();
        return { left: own.left, right: own.right, width: own.width, scrollWidth: element.scrollWidth, clientWidth: element.clientWidth, trackLeft: track?.left ?? 0, trackRight: track?.right ?? 0 };
      });
      expect(box.right, `${lane}: 右端 ${String(box.right)} / 枠 ${String(box.trackRight)}`).toBeLessThanOrEqual(box.trackRight + 0.5);
      expect(box.left, `${lane}: 左端`).toBeGreaterThanOrEqual(box.trackLeft - 0.5);
      // 文字が省略されずに全部見えている（要素の中で切れていない）
      expect(box.scrollWidth, `${lane}: 文字が切れている`).toBeLessThanOrEqual(box.clientWidth + 1);
      expect(box.width).toBeGreaterThan(20);
    }
    // 印は版の開始日 (右端の近く) に付いている: 枠の右 15% の中
    const pin = await timeline.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark').last().evaluate(element => {
      const track = element.closest('.jt-track')?.getBoundingClientRect();
      const before = window.getComputedStyle(element, '::before');
      const own = element.getBoundingClientRect();
      return { pinLeft: own.left + parseFloat(before.left) + parseFloat(before.marginLeft), trackLeft: track?.left ?? 0, trackWidth: track?.width ?? 1 };
    });
    expect((pin.pinLeft - pin.trackLeft) / pin.trackWidth).toBeGreaterThan(0.85);
    // 印の線は文字の下に出し、文字に重ねない（全部の 本文・画像 の印で確かめる）
    const overlaps = await timeline.locator('.jt-mark').evaluateAll(elements => elements.map(element => {
      const own = element.getBoundingClientRect();
      const before = window.getComputedStyle(element, '::before');
      const range = document.createRange(); range.selectNodeContents(element);
      const text = range.getBoundingClientRect();
      const pinTop = own.top + parseFloat(before.top);
      return { label: element.textContent, pinTop, textBottom: text.bottom };
    }));
    expect(overlaps.length).toBeGreaterThan(0);
    for (const mark of overlaps) expect(mark.pinTop, `${String(mark.label)} の線が文字に重なる`).toBeGreaterThanOrEqual(mark.textBottom);
  });

  test('1100x623 で上の帯が 1 行に収まり、印を選ぶと「選んだ版」が画面内に出る', async ({ page }) => {
    await page.setViewportSize({ width: 1100, height: 623 });
    await open(page);
    const topline = await page.locator('.jc-topline').evaluate(element => element.getBoundingClientRect().height);
    expect(topline).toBeLessThan(60);
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    await timeline.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark').last().click();
    const panel = timeline.getByRole('region', { name: '選んだ版', exact: true });
    await expect(panel).toContainText('2026/08/20時点の求人内容');
    await expect(panel).toBeInViewport({ ratio: 0.98 });
    // 期間比較表の行も「選択中」と文字で示す
    await expect(timeline.locator('tr[aria-current="true"]')).toContainText('選択中');
  });

  test('市場データが 2026-08 で終わり、期間がそれより先まで続くとき、データのある月で比べてそれ以降は「データなし」と示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page, '2026-10-05T00:00:00Z');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows).toHaveCount(2);
    // 1 つ目の期間は 07-01〜10-04。07 と 08 の市場データで比べ、09 以降はデータなしと書く。
    await expect(rows.nth(0).locator('td').nth(0)).toHaveText('96日');
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('+4.5%（2026/07 220件 → 2026/08 230件、2026/09以降はデータなし）');
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('データなし');
    await expect(timeline).toContainText('2026/09以降は市場データがありません（2026/08まで）');
    await expect(timeline.getByRole('group', { name: '市場', exact: true }).locator('.jt-nodata')).toHaveText('データなし');
    const market = await seriesLengths(page, 'jt-market');
    expect(market).toEqual([4, 4]);
    // 年月の書き方は YYYY/MM にそろえる（2026-08 や 2026年08月 を出さない）
    await expect(timeline).not.toContainText(/\d{4}-\d{2}(?!-)|\d{4}年\d{2}月/u);
  });

  test('市場データの取得に失敗したら期間比較表にもそう書き、再取得すると具体値とグラフが出る', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    const calls = await open(page, defaultCapturedAt, { seriesFailures: 1 });
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(timeline.getByRole('group', { name: '市場', exact: true })).toContainText('市場データを取得できませんでした');
    // 「市場を選ぶと表示」「データなし」とは書かない（選んだのに取れなかったことが分かるように）
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('取得できませんでした');
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('取得できませんでした');
    await timeline.getByRole('button', { name: '市場データを再取得', exact: true }).click();
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('+4.5%（2026/07 220件 → 2026/08 230件）');
    await expect(timeline.getByLabel('職種')).toHaveValue('配送ドライバー');
    const market = await seriesLengths(page, 'jt-market');
    expect(market).toEqual([2, 2]);
    // 再取得は選んだ市場の月次だけを取り直す（一覧は 1 回）
    expect(calls.filter(search => !search.includes('title='))).toHaveLength(1);
    expect(calls.filter(search => search.includes('title='))).toHaveLength(2);
  });

  test('課金CSVを読み込むと課金レーンと期間比較表に入り、再読み込みで消えることを示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    const csv = '媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,12345678,2026-07-01,2026-07-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing')).toHaveText(['3万1,000円']);
    await expect(timeline.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。', { exact: true })).toBeVisible();
    // 07-01〜07-31 の 31 日分 31,000円 のうち、1 つ目の期間（07-01〜08-19 の 50 日）に入るのは全額
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('3万1,000円');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('この期間の課金データなし');
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
  });

  test('課金を日数で配分した「約」と本文の印の読み上げを、ホバーしなくても読める文字で示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    // 08-01〜08-31 の 31 日で 31,000円。1 つ目の期間（〜08-19）に 19 日分、2 つ目（08-20 の 1 日）に 1 日分を配分する。
    const csv = '媒体,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,12345678,2026-08-01,2026-08-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const table = timeline.getByRole('region', { name: '期間比較表の数値' });
    const rows = table.locator('tbody tr');
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('約1万9,000円');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('約1,000円');
    await expect(timeline.getByText('「約」の付いた課金額は、課金の期間と版の期間がずれているため、日数で割って配分した金額です。', { exact: true })).toBeVisible();
    await expect(table.locator('[title]')).toHaveCount(0);
    // 本文の印: 画面の文字と読み上げが同じことを言う
    const marks = await timeline.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark').evaluateAll(elements => elements.map(element => [element.textContent, element.getAttribute('aria-label')]));
    expect(marks).toEqual([['最初', '2026/07/01時点の求人内容の本文：最初の版'], ['追加1・削除1', '2026/08/20時点の求人内容の本文：1行追加・1行削除']]);
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
    await expect(page.locator('[data-testid="jt-market"][data-chart-ready="true"]')).toHaveCount(1);
    // 求人内容でも、内部の呼び方（媒体取得版・受信版・確定対応・推定対応）を出さない
    await page.getByRole('tablist', { name: '求人管理の機能', exact: true }).getByRole('tab', { name: '求人内容', exact: true }).click();
    await expect(page.locator('.jc-reading')).toBeVisible();
    await expect(page.locator('body')).not.toContainText(/媒体取得版|受信版|確定対応|推定対応/u);
    await expect(page.locator('.jc-mode')).not.toHaveAttribute('title', /.+/u);
  });
});
