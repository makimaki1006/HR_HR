import { expect, test, type Page } from '@playwright/test';
import { login } from './helpers/login';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { closeOtherAnalysis, openOtherAnalysis, selectJobFeature } from '../job-copy-navigation';

/**
 * 案件管理 (/app/job-copy) を Rust が配る React 画面で確かめる。
 * 求人と応募の取得 (/api/job-copy/moc) は Drive の保存データを読むので、PR の fixture には無い。
 * ここでは合成データを page.route で返し、市場 (/api/job-copy/market) も合成の月次データを返す。
 * 確かめること: 最初のタブがタイムライン、7 レーンの値、2 つの ECharts の描画完了、期間比較表の具体値、
 * 課金CSV を読み込むと課金レーンと期間比較表に入ること、取込と逆検索が主作業の外にあること。
 * 本文は文面を確認した日で区切る（文面を確認した日A〜Bの間は前後どちらの本文にも入れない）。仮の課金データは合計しない。
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

/**
 * 画面を開き、市場と比べる職種を指定する。タイムラインなど旧タブは「ほかの分析」の横画面の中にある。
 * 既定では最後に横画面を閉じる（ページ本体の操作は横画面が開いている間はできないため）。
 * 再取得の回数を数える試験など、閉じて開き直すと取得が増える試験は keepOpen で開いたままにする。
 */
async function open(page: Page, capturedAt = defaultCapturedAt, options: { seriesFailures?: number; keepOpen?: boolean } = {}) {
  const calls: string[] = [];
  let seriesFailures = options.seriesFailures ?? 0;
  // 市場の職種は人が指定するまで空。指定している間は月次の取得を止め、指定後に画面の「再読み込み」で取り直す。
  let designating = false;
  await page.route('**/api/job-copy/moc', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(snapshot(capturedAt)) }));
  await page.route('**/api/job-copy/market*', route => {
    const url = new URL(route.request().url());
    if (designating && url.searchParams.has('title')) return route.abort();
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
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-job')).toHaveCount(1);
  // 職種を指定する（HubSpot への保存は合成の応答）。指定前は市場を取りにいかないことも確かめる。
  await page.route('**/api/job-copy/listings/30/indeed-title', route => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ listing_id: '30', indeed_title: '配送ドライバー', indeed_title_state: 'set' }) }));
  const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
  await openOtherAnalysis(page, 'タイムライン');
  await expect(timeline).toContainText('市場と比べる職種が未指定です');
  expect(calls.filter(search => search.includes('title='))).toHaveLength(0);
  // 職種の指定は右の「数字の前提」の欄から行う（横画面が開いている間は操作できないので閉じる）
  await closeOtherAnalysis(page);
  await page.getByRole('button', { name: '職種を指定する', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: '市場と比べる職種を指定する' });
  await drawer.getByLabel('配送ドライバー', { exact: true }).check();
  calls.length = 0;
  designating = true;
  await drawer.getByRole('button', { name: 'この職種を指定する', exact: true }).click();
  await expect(drawer).toBeHidden();
  await openOtherAnalysis(page, 'タイムライン');
  await expect(timeline.getByRole('button', { name: '市場データを再読み込み', exact: true })).toBeVisible();
  designating = false;
  await timeline.getByRole('button', { name: '市場データを再読み込み', exact: true }).click();
  if (!options.keepOpen) await closeOtherAnalysis(page);
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

test.describe('案件管理のタイムライン', () => {
  test('最初に週ごとの応募と出来事が出て、ほかの分析（旧タブ）の 7 レーン・グラフ・期間比較表に具体値が出る', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 800 });
    const calls = await open(page);
    // 最初の画面: 週ごとの応募のグラフと、出来事（新しい順）の一覧。旧タブは主領域に出ていない
    await expect(page.getByRole('heading', { name: '求人のライフライン', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: '出来事（新しい順）', exact: true })).toBeVisible();
    await expect(page.getByTestId('lifeline')).toBeVisible();
    await expect(page.getByRole('tablist', { name: '求人管理の機能', exact: true })).toHaveCount(0);
    await expect(page.getByRole('region', { name: 'タイムライン', exact: true })).toHaveCount(0);
    // 出来事は新しい順: 2026/08/20 の給与の変更が先で、2026/07/01 の掲載開始が後
    const eventHeads = page.locator('.lo-event:not(.lo-kind-application) .lo-event-head');
    await expect(eventHeads).toHaveCount(2);
    await expect(eventHeads.nth(0)).toContainText('給与を変更');
    await expect(eventHeads.nth(1)).toContainText('掲載を開始');
    // ここから「ほかの分析」の横画面（旧タブ全体）の中を確かめる
    await openOtherAnalysis(page, 'タイムライン');
    const primary = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
    await expect(primary.getByRole('tab')).toHaveText(['タイムライン', '本文・画像', '変更差分', '求人比較', '応募者構成', '応募理由']);
    await expect(primary.getByRole('tab', { name: 'タイムライン', exact: true })).toHaveAttribute('aria-selected', 'true');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    for (const lane of ['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場']) await expect(timeline.getByRole('group', { name: lane, exact: true })).toBeVisible();
    const salary = timeline.getByRole('group', { name: '給与', exact: true });
    await expect(salary).toContainText('月給23万〜26万円');
    await expect(salary).toContainText('月給25万〜28万円');
    // HRハッカー実績も課金CSVも無いので、仮の課金データ（ダミー）を月ごとに出す（求人IDで決まる金額）。
    // 07 月は 73,000円、08 月は 01〜20 日（応募の文面を確認した日まで）の 20/31 で 47,097円。
    const billingLane = timeline.getByRole('group', { name: '課金', exact: true });
    await expect(billingLane).toContainText('仮の課金データ（ダミー）');
    await expect(billingLane.locator('.jt-billing')).toHaveText(['ダミー 7万3,000円', 'ダミー 4万7,097円']);
    await expect(billingLane.locator('.jt-billing-dummy')).toHaveCount(2);
    await expect(timeline.getByRole('note').filter({ hasText: '実際の請求額ではありません' })).toBeVisible();
    await expect(timeline).toContainText('応募日が分からない応募 1件 はグラフに含めていません');
    // 市場は人が指定した職種と、勤務地から選んだ都道府県で取る
    await expect(timeline.getByText('市場と比べる職種：配送ドライバー（人が指定）', { exact: true })).toBeVisible();
    await expect(timeline.getByLabel('都道府県')).toHaveValue('大分県');
    expect(calls.some(search => search.includes('title=%E9%85%8D%E9%80%81%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC'))).toBe(true);
    const applications = await seriesLengths(page, 'jt-applications');
    expect(applications.length).toBeGreaterThan(0);
    expect(applications[0]).toBeGreaterThan(0);
    const market = await seriesLengths(page, 'jt-market');
    expect(market.length).toBe(2);
    expect(market[0]).toBeGreaterThan(0);
    // 期間比較表: 文面を確認した日 07-01 と 08-20 の行と、その間（給与が変わった。どちらの内容か分からない）の行。
    // 掲載日は分からず、応募は日付だけなので、変化の前後の文面を確認した日そのものもどちらの内容か分からない。
    // 本文の行は 0 日で、文面を確認した日 08-20 の応募も間の行に数える（07-01 は最初の文面を確認した日で、表より前に数える）。
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0).locator('th')).toContainText('2026/07/01に確認した本文');
    await expect(rows.nth(1).locator('th')).toContainText('文面を確認した日2026/07/01〜2026/08/20の間に変化');
    await expect(rows.nth(1).locator('th')).toContainText('2026/07/02〜2026/08/20（どちらの内容か分からない期間。文面を確認した日 2026/08/20 を含む）');
    await expect(rows.nth(2).locator('th')).toContainText('2026/08/20に確認した本文');
    // 仮の課金データは課金の段にだけ出し、期間比較表では合計しない
    const noRealBilling = '実際の課金データなし（仮の課金データ（ダミー）は合計しません）';
    await expect(rows.nth(0).locator('td')).toHaveText(['0日', '別の行に数えます', '期間が短いため比べません', '—', '同じ月の中（2026年7月 220件）']);
    await expect(rows.nth(1).locator('td')).toHaveText(['50日', '5件', '比べません', noRealBilling, '+4.5%（2026年7月 220件 → 2026年8月 230件）']);
    await expect(rows.nth(2).locator('td')).toHaveText(['0日', '別の行に数えます', '期間が短いため比べません', '—', '同じ月の中（2026年8月 230件）']);
    await expect(timeline.getByRole('region', { name: '期間比較表の数値' })).not.toContainText(/11万7,742円|2,355円/u);
    await expect(timeline).toContainText('文面を確認した日の間・最後の確認より後の応募 5件 は、どちらの内容への応募か分からないため期間比較表の各本文には入れていません');
    // 掲載期間の段: 文面を確認した日の間を別の帯で示し、凡例は「掲載日は不明（文面を確認した日で表示）」
    const periodLane = timeline.getByRole('group', { name: '掲載期間', exact: true });
    await expect(periodLane.locator('.jt-zone')).toHaveCount(1);
    await expect(periodLane.locator('.jt-zone')).toHaveAttribute('title', '文面を確認した日2026/07/01〜2026/08/20の間に変化。どちらの内容か分からない期間です');
    await expect(timeline.getByRole('group', { name: '凡例', exact: true })).toContainText('掲載日は不明（文面を確認した日で表示）');
    await expect(timeline).not.toContainText(/変更日|本文が切り替わった日|掲載日の確かさ/u);
    await expect(page.locator('body')).not.toContainText(/過去CSVの本文|媒体CSV取得本文|接続待ち|実求人ID|原本不足/u);
    // ここから先は画面本体の操作なので、横画面を閉じる（開いている間は本体を操作できない）
    await closeOtherAnalysis(page);
    // 上の帯の件数は期間比較表と同じ割り当てで数える（全 6 件のうち、本文の行に入った応募は無い）
    await page.locator('.jc-snapshot-tip > summary').click();
    const summary = page.getByRole('region', { name: '取り込んだ内容の内訳' });
    await expect(summary).toContainText('6応募（HubSpot記録分・求人ごとの件数の合計（重複あり））');
    await expect(summary).toContainText('どの本文への応募か分からない応募 6件（求人ごとの件数の合計（重複あり））');
    await expect(summary).toContainText('期間比較表の本文の行に入らない応募 6件（求人ごとの件数の合計（重複あり））');
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
    await openOtherAnalysis(page, 'タイムライン');
    await expect(billingLane.locator('.jt-billing')).toHaveCount(0);
    await expect(billingLane).toContainText('課金データなし（0円という意味ではありません）');
    await closeOtherAnalysis(page);
    await page.reload();
    await expect(page.getByRole('checkbox', { name: '仮の課金データを表示', exact: true })).not.toBeChecked();
    await page.getByRole('checkbox', { name: '仮の課金データを表示', exact: true }).check();
    await openOtherAnalysis(page, 'タイムライン');
    await expect(page.getByRole('region', { name: 'タイムライン', exact: true }).getByRole('group', { name: '課金', exact: true }).locator('.jt-billing-dummy')).toHaveCount(2);
    // 保存先は合成の応答なので、再読み込みで職種は未指定に戻る。指定し直すと市場が出る
    await expect(timeline).toContainText('市場と比べる職種が未指定です');
    await closeOtherAnalysis(page);
    await page.getByRole('button', { name: '職種を指定する', exact: true }).click();
    const drawer = page.getByRole('dialog', { name: '市場と比べる職種を指定する' });
    await drawer.getByLabel('配送ドライバー', { exact: true }).check();
    await drawer.getByRole('button', { name: 'この職種を指定する', exact: true }).click();
    await expect(drawer).toBeHidden();
    await openOtherAnalysis(page, 'タイムライン');
    // 市場の選び方: 職種は人が指定、都道府県は勤務地から自動
    await expect(timeline.getByText('市場と比べる職種：配送ドライバー（人が指定）', { exact: true })).toBeVisible();
    await expect(timeline.getByText('勤務地から大分県を自動で選びました', { exact: true })).toBeVisible();
    await closeOtherAnalysis(page);
    // 主作業の外: データ取込は閉じていて、逆検索は押すまで出ない。旧来の操作バーも無い。
    await expect(page.locator('.jc-data-import')).toBeHidden();
    await expect(page.locator('.jc-reverse-search')).toHaveCount(0);
    await expect(page.getByRole('button', { name: '求人一覧に戻る', exact: true })).toHaveCount(0);
    const firstView = await page.evaluate(() => ({ inner: window.innerHeight, list: document.querySelector('.jc-list')?.getBoundingClientRect().bottom ?? 0 }));
    expect(firstView.list).toBeGreaterThanOrEqual(firstView.inner - 2);
    await expect(page.locator('body')).not.toContainText(/効果|確実に|必ず|100%/u);
  });

  test('最新の本文の給与・本文・画像の印が右端で切れずに読める', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 800 });
    await open(page, defaultCapturedAt, { keepOpen: true });
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    // 最新の本文は文面を確認した日 (08-20) に始まり、時間軸の右端 (約 99%) に来る。
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
    // 印は本文の開始日 (右端の近く) に付いている: 枠の右 15% の中
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

  test('1100x623 で上の帯が 1 行に収まり、印を選ぶと「選んだ本文」が画面内に出る', async ({ page }) => {
    await page.setViewportSize({ width: 1100, height: 623 });
    await open(page, defaultCapturedAt, { keepOpen: true });
    const topline = await page.locator('.jc-topline').evaluate(element => element.getBoundingClientRect().height);
    expect(topline).toBeLessThan(60);
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    await timeline.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark').last().click();
    const panel = timeline.getByRole('region', { name: '選んだ本文', exact: true });
    await expect(panel).toContainText('2026/08/20時点の求人内容');
    await expect(panel).toBeInViewport({ ratio: 0.98 });
    await expect(panel).toContainText('2026/08/20 に確認（前回の確認 2026/07/01 以降に変化）');
    // 期間比較表の行も「選択中」と文字で示す
    await expect(timeline.locator('tr[aria-current="true"]')).toContainText('選択中');
  });

  test('市場データ（毎月更新）が 2026-08 で終わり、期間がそれより先まで続くとき、データのある月で比べてそれ以降は「データなし」と示す', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 800 });
    await open(page, '2026-10-05T00:00:00Z', { keepOpen: true });
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    // 文面を確認した日は 07-01 と 10-05。その間（07-02〜10-05、文面を確認した日 10-05 を含む）は給与が変わった期間で、どちらの内容か分からない。
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(1).locator('th')).toContainText('文面を確認した日2026/07/01〜2026/10/05の間に変化');
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
    await page.setViewportSize({ width: 1440, height: 800 });
    const calls = await open(page, defaultCapturedAt, { seriesFailures: 1, keepOpen: true });
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    await expect(timeline.getByRole('group', { name: '市場', exact: true })).toContainText('市場データを取得できませんでした');
    // 「市場を選ぶと表示」「データなし」とは書かない（選んだのに取れなかったことが分かるように）
    await expect(rows.nth(0).locator('td').nth(4)).toHaveText('市場データを確認できませんでした');
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('市場データを確認できませんでした');
    await timeline.getByRole('button', { name: '市場データを再読み込み', exact: true }).click();
    await expect(rows.nth(1).locator('td').nth(4)).toHaveText('+4.5%（2026年7月 220件 → 2026年8月 230件）');
    await expect(timeline.getByText('市場と比べる職種：配送ドライバー（人が指定）', { exact: true })).toBeVisible();
    const market = await seriesLengths(page, 'jt-market');
    expect(market).toEqual([2, 2]);
    // 画面の本体（出来事の市場の確認）も同じ月次を読むため、月次の取得は 2 回以上になりうる。失敗 1 回のあと取り直して成功している
    expect(calls.filter(search => search.includes('title=')).length).toBeGreaterThanOrEqual(2);
  });

  test('課金CSVを読み込むと課金の段と期間比較表に入り、再読み込みで消えることを示す', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    // 媒体 + 店舗ID + 媒体求人ID の 3 つで結びつける（先頭の 0 も文字のまま比べる）
    const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,0042,12345678,2026-07-01,2026-07-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    await openOtherAnalysis(page, 'タイムライン');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    // 07 月は課金CSVの金額に置き換わり、仮の課金データ（ダミー）は CSV に無い 08 月分だけ残る
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing')).toHaveText(['3万1,000円', 'ダミー 4万7,097円']);
    await expect(timeline.getByRole('group', { name: '課金', exact: true }).locator('.jt-billing-csv')).toHaveCount(1);
    await expect(timeline.getByText('読み込んだ課金CSVはこの画面を開いている間だけ表示します。再読み込みすると消えます。', { exact: true })).toBeVisible();
    // 07-01〜07-31 の 31 日分 31,000円 を日数で配分: 文面を確認した日の間（07-02〜）に 30 日分。07-01 の行は
    // 内容の分かる日が無い（0 日）ので金額を書かない
    const rows = timeline.getByRole('region', { name: '期間比較表の数値' }).locator('tbody tr');
    // 実際の金額だけを書き、仮の金額は合計しない（7万5,742円 のような合計も、仮の金額も出さない）
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('—');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('約3万円（仮の課金データ（ダミー）は合計に入れていません）');
    await expect(rows.nth(2).locator('td').nth(3)).toHaveText('—');
    await expect(timeline.getByRole('region', { name: '期間比較表の数値' })).not.toContainText(/7万5,742円|4万4,742円/u);
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
    // 印刷する報告（比較・報告）も、反映した課金CSVを読む（画面と食い違わない）
    await selectJobFeature(page, 'report');
    const report = page.getByRole('region', { name: '顧客報告と検証記録', exact: true });
    await expect(report).toContainText('読み込んだ課金CSV：1行・2026/07/01〜2026/07/31・金額の分かる行の合計 3万1,000円');
    await expect(report).not.toContainText('課金情報は未取得');
  });

  test('課金を日数で配分した「約」と本文の印の読み上げを、ホバーしなくても読める文字で示す', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 800 });
    await open(page);
    await page.getByRole('button', { name: 'データ取込', exact: true }).click();
    // 08-01〜08-31 の 31 日で 31,000円。文面を確認した日の間（〜08-20、文面を確認した日 08-20 を含む）に 20 日分を配分する。
    const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\nHRハッカー,0042,12345678,2026-08-01,2026-08-31,31000\n';
    await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv, 'utf8') });
    await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
    await page.getByRole('button', { name: '一致した1行を課金として反映', exact: true }).click();
    await page.getByRole('region', { name: 'データ取込', exact: true }).getByRole('button', { name: '閉じる', exact: true }).click();
    await openOtherAnalysis(page, 'タイムライン');
    const timeline = page.getByRole('region', { name: 'タイムライン', exact: true });
    const table = timeline.getByRole('region', { name: '期間比較表の数値' });
    const rows = table.locator('tbody tr');
    // 08 月は課金CSVの金額。07 月は CSV に無いので仮の課金データ（ダミー）のまま課金の段にだけ出し、合計しない
    await expect(rows.nth(0).locator('td').nth(3)).toHaveText('—');
    await expect(rows.nth(1).locator('td').nth(3)).toHaveText('約2万円（仮の課金データ（ダミー）は合計に入れていません）');
    await expect(rows.nth(2).locator('td').nth(3)).toHaveText('—');
    await expect(timeline.getByText('「約」の付いた課金額は、課金の期間と本文の表示期間がずれているため、日数で割って配分した金額です。', { exact: true })).toBeVisible();
    await expect(table.locator('[title]')).toHaveCount(0);
    // 本文の印: 画面の文字と読み上げが同じことを言う
    const marks = await timeline.getByRole('group', { name: '本文', exact: true }).locator('.jt-mark').evaluateAll(elements => elements.map(element => [element.textContent, element.getAttribute('aria-label')]));
    expect(marks).toEqual([['最初', '2026/07/01時点の求人内容の本文：最初の本文'], ['追加1・削除1', '2026/08/20時点の求人内容の本文：1行追加・1行削除']]);
    await expect(page.locator('[data-testid="jt-applications"][data-chart-ready="true"]')).toHaveCount(1);
    await expect(page.locator('[data-testid="jt-market"][data-chart-ready="true"]')).toHaveCount(1);
    // 求人内容でも、内部の呼び方（媒体取得本文・受信本文・確定対応・推定対応）を出さない
    await selectJobFeature(page, 'body');
    await expect(page.locator('.jc-reading')).toBeVisible();
    await expect(page.locator('body')).not.toContainText(/媒体取得本文|受信本文|確定対応|推定対応/u);
    await expect(page.locator('.jc-mode')).not.toHaveAttribute('title', /.+/u);
  });

  test('最初の画面: 出来事の一覧・最新の出来事の印・展開時の 3 つの確認・応募者の前後の横画面・ほかの分析の入口', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await open(page);
    // 出来事（新しい順）: 合成データは 2026/08/20 の給与の変更と 2026/07/01 の掲載開始の 2 件
    const eventsSection = page.locator('section[aria-labelledby="lo-events-title"]');
    await expect(eventsSection.getByRole('heading', { name: '出来事（新しい順）', exact: true })).toBeVisible();
    const heads = eventsSection.locator('.lo-event-head');
    await expect(heads).toHaveCount(2);
    await expect(heads.nth(0)).toContainText('給与を変更');
    await expect(heads.nth(1)).toContainText('掲載を開始');
    // 上の帯の「最新の出来事」は新しい順の先頭と同じ出来事
    const chip = page.getByTestId('band-latest-event');
    await expect(chip).toBeVisible();
    await expect(chip).toContainText('給与を変更');
    // 出来事を開くと、応募を動かしそうな ほかのことの確認が出る（課金・ほかの変更は常に、市場は該当するときだけ）
    await heads.nth(0).click();
    await expect(heads.nth(0)).toHaveAttribute('aria-expanded', 'true');
    const checks = eventsSection.getByRole('list', { name: 'ほかに応募を動かしそうなこと' }).locator('li');
    const labels = await checks.locator('strong').allTextContents();
    expect(labels).toContain('課金');
    expect(labels).toContain('ほかの変更');
    for (const label of labels) expect(['課金', '市場', 'ほかの変更']).toContain(label);
    expect(labels.filter(label => label === '市場').length).toBeLessThanOrEqual(1);
    for (const mark of await checks.locator('.lo-mark').allTextContents()) expect(['小さい', '要注意', '未確認', '未指定']).toContain(mark);
    // 課金のデータが無い合成データなので、課金の確認は「未確認」
    await expect(checks.filter({ hasText: '課金' }).first().locator('.lo-mark')).toHaveText('未確認');
    await expect(eventsSection.getByRole('button', { name: 'この出来事の前と後の全文を比べる', exact: true })).toBeVisible();
    // 応募者・応募理由の前後は横から出る。Escape で閉じる
    await eventsSection.getByRole('button', { name: '応募者・応募理由の前後を見る', exact: true }).click();
    const who = page.getByRole('dialog', { name: '応募者・応募理由の前後', exact: true });
    await expect(who).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(who).toBeHidden();
    // 出来事の説明に、原因を決める言葉を使わない
    await expect(eventsSection).not.toContainText(/増えた|減った|効果/u);
    // ほかの分析は入口のボタン 1 つから開き、タブは 6 つ（13 → 6）
    const more = page.getByRole('region', { name: 'ほかの分析', exact: true });
    await expect(more.getByRole('button')).toHaveText(['ほかの分析を開く']);
    await expect(page.getByRole('dialog', { name: 'ほかの分析', exact: true })).toHaveCount(0);
  });
});

test('電話ヒアリングを開くとデモの聞き取りと通話記録を応募日と並べて読める', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await login(page);
  await page.goto('/app/job-copy?demo=1&job=demo-job-001');
  const section = page.locator('.ph-section');
  await expect(section.locator('summary')).toHaveText('電話ヒアリング（記録あり 3件 / 応募 28件）');
  await expect(section).not.toHaveAttribute('open');
  await section.locator('summary').click();
  const card = section.locator('article').first();
  await expect(card).toContainText('応募日：2026/09/24');
  await expect(card.locator('h3')).toHaveText('応募者：38・女性・大分県大分市');
  await expect(card.locator('.ph-note > p').first()).toHaveText('2026/09/24 13:30');
  await expect(section).not.toContainText('JST');
  await expect(card).toContainText('勤務条件の優先順位');
  await expect(card).toContainText('通勤時間、休日、給与の順で希望');
  await expect(card).toContainText('【架電】土日休みと、見学できる日程を確認したい。');
  await expect(section.locator('article').nth(1)).toContainText('ほかの求人にも関連する応募');
  await section.locator('summary').scrollIntoViewIfNeeded();
  if (process.env.PHONE_HEARING_SCREENSHOT) await page.screenshot({ path: process.env.PHONE_HEARING_SCREENSHOT, fullPage: true });
});

test('ホームは初回準備の2秒後にカードと顧客表を表示する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  let homeCalls = 0;
  const counts = { zero_stay: 2, to_zero: 1, less: 3, more: 8, flat: 12, costly: 2, stale: 1 };
  let changesReady = false;
  let changesCalls = 0;
  await page.route('**/api/job-copy/recent-changes', route => {
    changesCalls += 1;
    return route.fulfill({ json: changesReady
      ? { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } }
      : { status: 'preparing' } });
  });
  await page.route('**/api/job-copy/home', route => {
    homeCalls += 1;
    return route.fulfill({ json: homeCalls === 1 ? { status: 'preparing' } : {
      status: 'ready', index_built_at: '2026-10-15T00:00:00Z', today: '2026-10-15', companies_ok: true,
      rules: demoHome.rules,
      home: {
        cards: demoHome.home.cards.map(card => ({ ...card, count: counts[card.key], listings: [] })),
        customers: [
          { company_id: '101', name: 'デモ事業所A', deals: 3, active_deals: 2, latest_deal: { name: '配送スタッフ募集（デモ）', stage_label: '契約中', active: true, created_at: '2026-10-01' }, total_listings: 15, live_listings: 10, recent_apps: 24, prev_apps: 18, recent_cost: 60000, cost_per_app: 2500 },
          { company_id: '102', name: 'デモ事業所B', deals: 1, active_deals: 1, latest_deal: { name: '事務スタッフ募集（デモ）', stage_label: '契約中', active: true, created_at: '2026-09-25' }, total_listings: 8, live_listings: 6, recent_apps: 12, prev_apps: 14, recent_cost: 36000, cost_per_app: 3000 },
        ], live_without_company: 0, median_cost_per_app: 2500,
      },
    } });
  });
  await login(page);
  await page.goto('/app/job-copy');
  await expect(page.getByText('求人の一覧を準備しています', { exact: true })).toBeVisible();
  const cardCounts = page.locator('.jh-cards .jh-card .jh-count');
  const title = (key: string) => demoHome.home.cards.find(card => card.key === key)!.title;
  const moreLine = (recent: string) => `ほかの見るもの：${title('zero_stay')} 2件・${title('less')} 3件・${title('more')} 8件・${title('flat')} 12件・${title('stale')} 1件・直近7日に変更 ${recent}`;
  await expect(cardCounts).toHaveText(['1件', '2件'], { timeout: 4_000 });
  await expect(page.locator('.jh-more')).toHaveText(moreLine('集計中'));
  await expect(page.locator('.jh-customer-table')).toHaveCount(0);
  await page.getByRole('button', { name: /^すべて表示/ }).click();
  await expect(page.locator('.jh-customer-table tbody tr')).toHaveCount(2);
  await expect(page.locator('.jh-customer-table')).toContainText('18 → 24 件');
  await expect(page.locator('.jh-customer-table')).toContainText('¥2,500');
  expect(homeCalls).toBe(2);
  await page.getByRole('button', { name: /直近7日に変更/ }).click();
  const changes = page.getByRole('region', { name: '直近7日に変更の一覧' });
  await expect(changes.getByRole('status')).toHaveText('変更のあった求人を集計しています…');
  changesReady = true;
  await expect(page.locator('.jh-more')).toHaveText(moreLine('0件'), { timeout: 4_000 });
  await expect(changes.getByRole('heading')).toHaveText('直近7日に変更（0件）');
  await expect(changes).toContainText('該当する求人はありません。');
  expect(changesCalls).toBeGreaterThanOrEqual(2);
  expect(homeCalls).toBe(2);
  if (process.env.HOME_SPEED_SCREENSHOT) await page.screenshot({ path: process.env.HOME_SPEED_SCREENSHOT, fullPage: true });
});

test('ホームの準備中に読み終えた件数と失敗後の待ち時間を表示する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  let failed = false;
  await page.route('**/api/job-copy/home', route => route.fulfill({ json: {
    status: 'preparing', progress: { read_count: 12300, estimated_total: 41000, retry_after_seconds: failed ? 60 : null },
  } }));
  await login(page);
  await page.goto('/app/job-copy');
  await expect(page.getByText('約 41,000 件中 12,300 件まで読みました', { exact: true })).toBeVisible();
  if (process.env.PREPARING_PROGRESS_SCREENSHOT) await page.screenshot({ path: process.env.PREPARING_PROGRESS_SCREENSHOT, fullPage: true });
  failed = true;
  await page.getByRole('button', { name: '再読み込み', exact: true }).click();
  await expect(page.getByText('読み込みに失敗したため、1 分後にもう一度読み込みます。', { exact: true })).toBeVisible();
});


test('出来事の応募に属性と応募理由と電話ヒアリングが同じ枠にそろい、カードと往復できる', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await login(page);
  await page.goto('/app/job-copy?demo=1&job=demo-job-001');
  const event = page.locator(`[id="application-${'8'.repeat(64)}"]`);
  await event.locator('.lo-event-head').click();
  await expect(event).toContainText('女性 ・ 38 ・ 大分県大分市');
  await expect(event).toContainText('通勤に片道1時間かかるため');
  await expect(event).toContainText('勤務条件の優先順位');
  await expect(event).toContainText('通勤時間、休日、給与の順で希望');
  await expect(event).toContainText('【架電】土日休みと、見学できる日程を確認したい。');
  await event.scrollIntoViewIfNeeded();
  if (process.env.APPLICANT_EVENT_SCREENSHOT) await page.screenshot({ path: process.env.APPLICANT_EVENT_SCREENSHOT });
  await event.getByRole('button', { name: '電話ヒアリングのこの応募へ' }).click();
  const card = page.locator(`[id="hearing-${'8'.repeat(64)}"]`);
  await expect(card).toBeVisible();
  await expect(card).toContainText('応募者：38・女性・大分県大分市');
  if (process.env.APPLICANT_CARD_SCREENSHOT) await page.screenshot({ path: process.env.APPLICANT_CARD_SCREENSHOT });
  await event.locator('.lo-event-head').click();
  await card.getByRole('button', { name: 'タイムラインのこの応募へ' }).click();
  await expect(event.locator('.lo-event-head')).toHaveAttribute('aria-expanded', 'true');
});

test('ライフラインの変更印は週でまとまり、キーボードで最新の出来事を開く', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.clock.install({ time: new Date('2026-08-23T03:00:00Z') });
  const data = snapshot();
  const listing = data.capture_bundle.jobs[0]!;
  listing.history.push(
    { id: 'salary-change', capturedAt: '2026-08-10T00:00:00Z', body: listing.body, images: [] },
    { id: 'body-change', capturedAt: '2026-08-17T00:00:00Z', body: listing.body + '\n配送先は地域内です。', images: [] },
  );
  listing.body += '\n配送先は地域内です。研修があります。';
  await page.route('**/api/job-copy/**', route => route.request().url().endsWith('/moc')
    ? route.fulfill({ json: data })
    : route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } }));
  await login(page);
  await page.goto('/app/job-copy?data=actual');
  const chart = page.getByTestId('lifeline');
  const marks = chart.locator('.lf-change');
  await expect(marks).toHaveText(['給与変更', '変更×2']);
  await expect(marks.nth(0)).toHaveAttribute('aria-label', /2026-08-10 給与を変更/);
  await expect(marks.nth(1)).toHaveAttribute('title', /2026-08-17 本文を変更[\s\S]*2026-08-20 本文を変更/);
  await expect(chart.locator('.lf-changes .lf-cell').nth(7).locator('button')).toHaveText('変更×2');
  if (process.env.LIFELINE_SCREENSHOT) {
    await chart.screenshot({ path: process.env.LIFELINE_SCREENSHOT });
  }
  await marks.nth(1).focus();
  await page.keyboard.press('Enter');
  const opened = page.locator('.lo-event.lo-open');
  await expect(opened.locator('.lo-event-head')).toHaveAttribute('aria-expanded', 'true');
  await expect(opened.locator('.lo-date')).toHaveText('8/20');
  await expect(opened.locator('.lo-summary')).toContainText('本文を変更');
  await expect(opened).toBeInViewport();
  await expect(chart.getByTestId('lifeline-selected')).toHaveCSS('left', '392px');
  await marks.nth(0).focus();
  await page.keyboard.press('Space');
  await expect(page.locator('.lo-event.lo-open .lo-date')).toHaveText('8/10');
  if (process.env.LIFELINE_SCREENSHOT) {
    await page.clock.setSystemTime(new Date('2026-10-05T03:00:00Z'));
    await page.goto('/app/job-copy?demo=1&job=demo-job-001');
    await expect(chart.locator('.lf-change')).toHaveText(['本文・給与・画像変更', '本文・給与・画像変更']);
    await chart.screenshot({ path: process.env.LIFELINE_SCREENSHOT.replace(/\.png$/, '-demo.png') });
  }
});
