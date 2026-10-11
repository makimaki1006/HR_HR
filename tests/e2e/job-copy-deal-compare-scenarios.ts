import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
const body = (amount: number) => `求人id：hidden-media-id\n案件名：配送スタッフ\n仕事内容：日用品を決まったルートで届けます。\nIndeed表示職種名：ドライバー\n給与形態：月給\n基本給与 最小：${amount}\n基本給与 最大：320000\n勤務時間：8:00〜17:00\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：土日休み`;
export function registerDealCompareTests(path: string) {
  for (const width of [1440, 1920]) test(`末尾空白の比較と給与表示 ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
    const jobs = ['250000 ', '9000'].map((salary, index) => ({ id: String(index + 1), title: `空白確認の配送求人${index + 1}`, media: 'hrh', media_job_id: 'hidden-id', account_id: null, prefecture: '沖縄県', municipality: '那覇市', category: 'ドライバー', publication_status: '公開', application_count: null, summary: { salary_kind: '月給', salary_min: salary, salary_max: salary } }));
    await page.route('**/api/job-copy/**', async route => {
      const url = new URL(route.request().url());
      if (url.pathname.endsWith('/listing-comparison')) return route.fulfill({ json: { jobs: jobs.map((listing, index) => ({ listing, body: `仕事内容：配送${index ? ' ' : ''}` })) } });
      if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: jobs, total: 2, titles: [], deals: [{ id: '901', name: '合成物流の契約', count: 2 }], no_deal_count: 0, offset: 0, next_offset: null, index_built_at: '2026-10-11T00:00:00Z' } });
      return route.fulfill({ status: 503, json: {} });
    });
    await page.goto(path);
    const panel = page.getByRole('region', { name: 'HubSpot の求人' });
    await panel.getByText('絞り込み・並び順', { exact: true }).click();
    await panel.getByLabel('取引', { exact: true }).selectOption('901');
    await panel.getByRole('button', { name: 'この取引の求人', exact: true }).click();
    const table = page.getByRole('table', { name: '取引の求人一覧表' });
    await expect(table.locator('tbody tr').first()).toContainText('月給 250,000円');
    await expect(table.locator('tbody tr').last()).toContainText('月給 9,000円');
    for (const job of jobs) await table.getByLabel(`${job.title}を比較に選ぶ`).check();
    await page.getByRole('button', { name: '横に並べて比べる' }).click();
    const comparison = page.getByRole('region', { name: '選んだ求人の横並び比較' });
    await expect(comparison).toContainText('本文には違いがあります。');
    await expect(comparison).toContainText('違う項目（1項目）');
    await expect(comparison.locator('.jc-comparison-different')).toHaveCount(1);
    expect(await comparison.locator('.jc-comparison-different td').allTextContents()).toEqual(['配送', '配送 ']);
    await expect(comparison.locator('details')).not.toHaveAttribute('open');
    if (width === 1440) {
      await comparison.scrollIntoViewIfNeeded();
      await page.screenshot({ path: 'docs/screenshots/job-copy-deal-compare/04-whitespace-comparison-1440.png' });
    }
  });
  for (const width of [1440, 1920]) test(`取引の絞り込み・101件の表・横並び比較 ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
    const jobs = Array.from({ length: 103 }, (_, index) => ({ id: String(index + 1), title: `配送スタッフ${String(index + 1).padStart(3, '0')}`, media: index % 2 ? 'airwork' : 'hrh', media_job_id: `hidden-${index}`, account_id: null,
      prefecture: '沖縄県', municipality: '那覇市', category: 'ドライバー', publication_status: index % 2 ? '01' : '公開', last_csv_detected_at: null, application_count: index,
      deal_ids: index < 101 ? index === 0 ? ['901', '902'] : ['901'] : [], summary: { occupation: '配送ドライバー', location: '沖縄県那覇市', salary_kind: '月給', salary_min: String(250000 + index * 1000), salary_max: '320000', hours: '8:00〜17:00', holidays: '土日休み', body_changed_at: '2026-10-10T00:00:00Z' } }));
    const requests: URL[] = [];
    await page.route('**/api/job-copy/**', async route => {
      const url = new URL(route.request().url()); requests.push(url);
      if (url.pathname.endsWith('/market')) return route.fulfill({ json: { titles: [], prefectures: [], series: null } });
      if (url.pathname.endsWith('/listing-comparison')) return route.fulfill({ json: { jobs: (url.searchParams.get('ids') ?? '').split(',').map(id => ({ listing: jobs.find(row => row.id === id), body: body(250000 + (Number(id) - 1) * 1000) })) } });
      if (url.pathname.endsWith('/versions')) { const row = jobs.find(row => row.id === url.pathname.split('/').at(-2)); return route.fulfill({ json: { listing: row, versions: [{ body: body(250000), written_at: '2026-10-10T00:00:00Z', image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } }); }
      if (url.pathname.endsWith('/listings')) {
        let rows = jobs.filter(row => ['media', 'prefecture'].every(key => !url.searchParams.get(key) || row[key as 'media' | 'prefecture'] === url.searchParams.get(key)) && (!url.searchParams.get('title') || row.category === url.searchParams.get('title')) && (!url.searchParams.get('deal') || (url.searchParams.get('deal') === 'none' ? !row.deal_ids.length : row.deal_ids.includes(url.searchParams.get('deal') ?? ''))));
        if (url.searchParams.get('publication')) rows = rows.filter(row => (row.publication_status === '01' ? '不明' : row.publication_status) === url.searchParams.get('publication'));
        if (url.searchParams.get('sort') === 'salary_min') rows.sort((a, b) => Number(a.summary.salary_min) - Number(b.summary.salary_min));
        if (url.searchParams.get('descending') === 'true') rows.reverse();
        const offset = Number(url.searchParams.get('offset') ?? 0);
        const deals = [{ id: '901', name: '合成物流・那覇の契約', count: 101 }, { id: '902', name: '合成物流・別の契約', count: 1 }].filter(deal => !url.searchParams.get('deal_search') || deal.name.includes(url.searchParams.get('deal_search') ?? ''));
        return route.fulfill({ json: { status: 'ready', listings: rows.slice(offset, offset + 50), titles: ['ドライバー'], deals, no_deal_count: 2, total: rows.length, offset, next_offset: offset + 50 < rows.length ? offset + 50 : null, index_built_at: '2026-10-10T00:00:00Z', refreshing: false, refresh_failed: false } });
      }
      return route.fulfill({ status: 503, json: {} });
    });
    await page.goto(path);
    const panel = page.getByRole('region', { name: 'HubSpot の求人' });
    await panel.getByText('絞り込み・並び順', { exact: true }).click();
    await panel.getByLabel('取引名を検索').fill('那覇');
    await expect(panel.getByRole('option', { name: '合成物流・那覇の契約（101件）' })).toHaveCount(1);
    await panel.getByLabel('取引', { exact: true }).selectOption('901');
    await expect(panel.locator('.jc-hubspot-list-heading')).toContainText('101件');
    await panel.getByLabel('都道府県', { exact: true }).selectOption('沖縄県');
    await panel.getByLabel('職種の分類').selectOption('ドライバー');
    await expect.poll(() => requests.at(-1)?.searchParams.get('title')).toBe('ドライバー');
    expect(requests.at(-1)?.searchParams.get('deal')).toBe('901');
    await expect(panel.locator('.jc-publication').filter({ hasText: /^01$/ })).toHaveCount(0);
    if (width === 1440) { await mkdir('docs/screenshots/job-copy-deal-compare', { recursive: true }); await page.screenshot({ path: 'docs/screenshots/job-copy-deal-compare/01-deal-filter-1440.png' }); }
    await panel.getByRole('button', { name: 'この取引の求人', exact: true }).click();
    const table = page.getByRole('table', { name: '取引の求人一覧表' });
    await expect(table.getByRole('row')).toHaveCount(51);
    await expect(table.locator('tbody tr').first()).toContainText('月給 250,000円');
    if (width === 1440) await page.screenshot({ path: 'docs/screenshots/job-copy-deal-compare/02-deal-table-1440.png' });
    await page.getByLabel('表の媒体').selectOption('hrh');
    await expect.poll(() => requests.at(-1)?.searchParams.get('media')).toBe('hrh');
    await expect(page.locator('.jc-deal-comparison')).toContainText('51件');
    await page.getByLabel('表の公開状況').selectOption('公開');
    await expect.poll(() => requests.at(-1)?.searchParams.get('publication')).toBe('公開');
    await page.getByLabel('表の媒体').selectOption('');
    await expect(table.locator('tbody tr')).toHaveCount(50);
    await page.getByLabel('表の公開状況').selectOption('');
    await expect(page.locator('.jc-deal-comparison')).toContainText('101件');
    await table.getByRole('button', { name: '給与の下限', exact: true }).click();
    await expect.poll(() => requests.at(-1)?.searchParams.get('sort')).toBe('salary_min');
    await table.getByLabel('配送スタッフ001を比較に選ぶ').check();
    await table.getByLabel('配送スタッフ002を比較に選ぶ').check();
    await page.getByRole('button', { name: '次の50件' }).click();
    await expect(table.getByRole('button', { name: '配送スタッフ051', exact: true })).toBeVisible();
    await page.getByRole('button', { name: '次の50件' }).click();
    await expect(table.getByRole('row')).toHaveCount(2);
    await page.getByRole('button', { name: '横に並べて比べる' }).click();
    const comparison = page.getByRole('region', { name: '選んだ求人の横並び比較' });
    await expect(comparison.locator('.jc-comparison-different')).toHaveCount(1);
    await expect(comparison.locator('.jc-comparison-different')).toContainText('月給 250,000円');
    await expect(comparison.locator('.jc-comparison-different')).toContainText('月給 251,000円');
    await expect(comparison.locator('details')).not.toHaveAttribute('open');
    await comparison.scrollIntoViewIfNeeded();
    if (width === 1440) await page.screenshot({ path: 'docs/screenshots/job-copy-deal-compare/03-parallel-1440.png' });
    await comparison.getByText(/^同じ項目/).click();
    await expect(comparison.locator('details')).toHaveAttribute('open');
    await expect(comparison).not.toContainText('hidden-media-id');
    await expect(comparison).not.toContainText('基本給与 最小');
    await page.getByRole('button', { name: '前の50件' }).click();
    await table.getByRole('button', { name: '配送スタッフ051', exact: true }).click();
    await expect(page.locator('.jc-main')).toContainText('HubSpotに記録された本文');
  });
}
