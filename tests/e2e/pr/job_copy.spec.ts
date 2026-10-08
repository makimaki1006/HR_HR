import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 求人文面管理 (/app/job-copy) を Rust が配る React 画面で確かめる。
 * 求人と応募の取得 (/api/job-copy/moc) は Drive の保存データを読むので、PR の fixture には無い。
 * ここでは合成データを page.route で返し、市場 (/api/job-copy/market) も合成の月次データを返す。
 * 確かめること: 最初のタブがタイムライン、7 レーンの値、2 つの ECharts の描画完了、期間比較表の具体値、
 * 課金CSV を読み込むと課金レーンと期間比較表に入ること、取込と逆検索が主作業の外にあること。
 * 版は取得日で区切る（取得日A〜Bの間は前後どちらの版にも入れない）。仮の課金データは合計しない。
 */
const defaultCapturedAt = '2026-08-20T00:00:00Z';
const months = Array.from({ length: 14 }, (_, index) => `${2025 + Math.floor((index + 6) / 12)}-${String((index + 6) % 12 + 1).padStart(2, '0')}`);

function snapshot(capturedAt = defaultCapturedAt) {
  return {
    schemaVersion: 1, capturedAt,
    capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{
      id: 'synthetic-pr-timeline', hubspotListingId: '30', shopId: '0042', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678',
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
    // HRハッカー実績も課金CSVも無いので、仮の課金データ（ダミー）を月ごとに出す（求人IDで決まる金額）。
    // 07 月は 73,000円、08 月は 01〜20 日（応募の取得日まで）の 20/31 で 47,097円。
    const billingLane = timeline.getByRole('group', { name: '課金', exact: true });
    await expect(billingLane).toContainText('仮の課金データ（ダミー）');
    await expect(billingLane.locator('.jt-billing')).toHaveText(['ダミー 7万3,000円', 'ダミー 4万7,097円']);
    await expect(billingLane.locator('.jt-billing-dummy')).toHaveCount(2);
    await expect(timeline.getByRole('note').filter({ hasText: '実際の請求額ではありません' })).toBeVisible();
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
    // 期間比較表: 取得日 07-01 と 08-20 の行と、その間（給与が変わった。どちらの内容か分からない）の行。
    // 掲載日は分からず、応募は日付だけなので、変化の前後の取得日そのものもどちらの内容か分からない。
    // 版の行は 0 日で、取得日 08-20 の応募も間の行に数える（07-01 は最初の取得日で、表より前に数える）。
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0).locator('th')).toContainText('2026/07/01に取得した内容');
    await expect(rows.nth(1).locator('th')).toContainText('取得日2026/07/01〜2026/08/20の間に変化');
    await expect(rows.nth(1).locator('th')).toContainText('2026/07/02〜2026/08/20（どちらの内容か分からない期間。取得した日 2026/08/20 を含む）');
    await expect(rows.nth(2).locator('th')).toContainText('2026/08/20に取得した内容');
    // 仮の課金データは課金の段にだけ出し、期間比較表では合計しない
    const noRealBilling = '実際の課金データなし（仮の課金データ（ダミー）は合計しません）';
    await expect(rows.nth(0).locator('td')).toHaveText(['0日', '別の行に数えます', '期間が短いため比べません', '—', '同じ月の中（2026年7月 220件）']);
    await expect(rows.nth(1).locator('td')).toHaveText(['50日', '5件', '比べません', noRealBilling, '+4.5%（2026年7月 220件 → 2026年8月 230件）']);
    await expect(rows.nth(2).locator('td')).toHaveText(['0日', '別の行に数えます', '期間が短いため比べません', '—', '同じ月の中（2026年8月 230件）']);
    await expect(timeline.getByRole('region', { name: '期間比較表の数値' })).not.toContainText(/11万7,742円|2,355円/u);
    await expect(timeline).toContainText('取得日の間・最後の取得より後の応募 5件 は、どちらの内容への応募か分からないため期間比較表の各版には入れていません');
    // 掲載期間の段: 取得日の間を別の帯で示し、凡例は「掲載日は不明（取得日で表示）」
    const periodLane = timeline.getByRole('group', { name: '掲載期間', exact: true });
    await expect(periodLane.locator('.jt-zone')).toHaveCount(1);
    await expect(periodLane.locator('.jt-zone')).toHaveAttribute('title', '取得日2026/07/01〜2026/08/20の間に変化。どちらの内容か分からない期間です');
    await expect(timeline.getByRole('group', { name: '凡例', exact: true })).toContainText('掲載日は不明（取得日で表示）');
    await expect(timeline).not.toContainText(/変更日|版が切り替わった日|掲載日の確かさ/u);
    await expect(page.locator('body')).not.toContainText(/過去CSVの版|媒体CSV取得版|接続待ち|実求人ID|原本不足/u);
    // 上の帯の件数は期間比較表と同じ割り当てで数える（全 6 件のうち、版の行に入った応募は無い）
    await page.locator('.jc-snapshot-tip > summary').click();
    const summary = page.getByRole('region', { name: '実データの取得範囲' });
    await expect(summary).toContainText('6応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
    await expect(summary).toContainText('どの版への応募か分からない応募 6件（求人ごとの件数の合計（重複あり））');
    await expect(summary).toContainText('期間比較表の版の行に入らない応募 6件（求人ごとの件数の合計（重複あり））');
    await page.locator('.jc-snapshot-tip > summary').click();
    // 一部の求人だけを表示していることを、上の帯の下に 1 行で示す
    await expect(page.getByText('この画面は、選んで取り込んだ一部の求人（1件）だけを表示しています。管理しているすべての求人ではありません。', { exact: true })).toBeVisible();
    // 「仮の課金データを表示」を外すと、課金レーンは「課金データなし（0円という意味ではありません）」
    const toggle = page.getByRole('checkbox', { name: '仮の課金データを表示', exact: true });
    await expect(toggle).toBeChecked();
    // 実データの帯でも、課金額が仮の金額であることを書く（外すと消える）
    await expect(page.locator('.jc-demo')).toContainText('課金額は、実際の課金データがまだ無いため仮の金額（ダミー）を表示しています。実際の請求額ではありません');
    await toggle.uncheck();
    await expect(page.locator('.jc-demo')).not.toContainText('ダミー');
    await expect(billingLane.locator('.jt-billing')).toHaveCount(0);
    await expect(billingLane).toContainText('課金データなし（0円という意味ではありません）');
    await page.reload();
    await expect(page.getByRole('checkbox', { name: '仮の課金データを表示', exact: true })).not.toBeChecked();
    await page.getByRole('checkbox', { name: '仮の課金データを表示', exact: true }).check();
    await expect(page.getByRole('region', { name: 'タイムライン', exact: true }).getByRole('group', { name: '課金', exact: true }).locator('.jt-billing-dummy')).toHaveCount(2);
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
    await expect(panel).toContainText('2026/08/20 に取得（前回の取得 2026/07/01 以降に変化）');
    // 期間比較表の行も「選択中」と文字で示す
    await expect(timeline.locator('tr[aria-current="true"]')).toContainText('選択中');
  });

  test('市場データ（毎月更新）が 2026-08 で終わり、期間がそれより先まで続くとき、データのある月で比べてそれ以降は「データなし」と示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page, '2026-10-05T00:00:00Z');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    // 取得日は 07-01 と 10-05。その間（07-02〜10-05、取得日 10-05 を含む）は給与が変わった期間で、どちらの内容か分からない。
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(1).locator('th')).toContainText('取得日2026/07/01〜2026/10/05の間に変化');
    await expect(rows.nth(1).locator('td').nth(0)).toHaveText('96日');
    await expect(rows.nth(1).locator('td').nth(2)).toHaveText('比べません');
    // 07 と 08 の市場データで比べ、09 以降はデータなしと書く。
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('+4.5%（2026年7月 220件 → 2026年8月 230件、2026年9月以降はデータなし）');
    // 期間がまるごと市場データより後: どの月までデータがあるかを書く（最後の月はデータから読む）
    await expect(rows.nth(2).locator('td').nth(4)).toHaveText('データなし（市場求人数は2026年8月まで）');
    await expect(timeline).toContainText('市場データは2026年8月まで（毎月更新）。2026年9月以降はデータなしとして表示しています');
    await expect(timeline.getByRole('group', { name: '市場', exact: true }).locator('.jt-nodata')).toHaveText('データなし');
    const market = await seriesLengths(page, 'jt-market');
    expect(market).toEqual([4, 4]);
    // 年月の書き方は YYYY/MM にそろえる（2026-08 や 2026年08月 を出さない）
    await expect(timeline).not.toContainText(/\d{4}-\d{2}(?!-)|\d{4}年0\d月/u);
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
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('+4.5%（2026年7月 220件 → 2026年8月 230件）');
    await expect(timeline.getByLabel('職種')).toHaveValue('配送ドライバー');
    const market = await seriesLengths(page, 'jt-market');
    expect(market).toEqual([2, 2]);
    // 再取得は選んだ市場の月次だけを取り直す（一覧は 1 回）
    expect(calls.filter(search => !search.includes('title='))).toHaveLength(1);
    expect(calls.filter(search => search.includes('title='))).toHaveLength(2);
  });

  test('課金CSVを読み込むと課金の段と期間比較表に入り、再読み込みで消えることを示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    // 媒体 + 店舗ID + 媒体求人ID の 3 つで結びつける（先頭の 0 も文字のまま比べる）
    const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,0042,12345678,2026-07-01,2026-07-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    // 07 月は課金CSVの金額に置き換わり、仮の課金データ（ダミー）は CSV に無い 08 月分だけ残る
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing')).toHaveText(['3万1,000円', 'ダミー 4万7,097円']);
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing-csv')).toHaveCount(1);
    await expect(timeline.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。', { exact: true })).toBeVisible();
    // 07-01〜07-31 の 31 日分 31,000円 を日数で配分: 取得日の間（07-02〜）に 30 日分。07-01 の行は
    // 内容の分かる日が無い（0 日）ので金額を書かない
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    // 実際の金額だけを書き、仮の金額は合計しない（7万5,742円 のような合計も、仮の金額も出さない）
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('—');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('約3万円（仮の課金データ（ダミー）は合計に入れていません）');
    await expect(rows.nth(2).locator('td').nth(3)).toHaveText('—');
    await expect(timeline.getByRole('region', { name: '期間比較表の数値' })).not.toContainText(/7万5,742円|4万4,742円/u);
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
    // 印刷する報告（比較・報告）も、反映した課金CSVを読む（画面と食い違わない）
    await page.getByRole('tablist', { name: '求人管理の機能', exact: true }).getByRole('tab', { name: '比較・報告', exact: true }).click();
    await page.getByRole('tab', { name: '顧客報告・検証', exact: true }).click();
    const report = page.getByRole('region', { name: '顧客報告と検証記録', exact: true });
    await expect(report).toContainText('読み込んだ課金CSV：1行・2026/07/01〜2026/07/31・金額の分かる行の合計 3万1,000円');
    await expect(report).not.toContainText('課金情報は未取得');
  });

  test('課金を日数で配分した「約」と本文の印の読み上げを、ホバーしなくても読める文字で示す', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    // 08-01〜08-31 の 31 日で 31,000円。取得日の間（〜08-20、取得日 08-20 を含む）に 20 日分を配分する。
    const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,0042,12345678,2026-08-01,2026-08-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const table = timeline.getByRole('region', { name: '期間比較表の数値' });
    const rows = table.locator('tbody tr');
    // 08 月は課金CSVの金額。07 月は CSV に無いので仮の課金データ（ダミー）のまま課金の段にだけ出し、合計しない
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('—');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('約2万円（仮の課金データ（ダミー）は合計に入れていません）');
    await expect(rows.nth(2).locator('td').nth(3)).toHaveText('—');
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
