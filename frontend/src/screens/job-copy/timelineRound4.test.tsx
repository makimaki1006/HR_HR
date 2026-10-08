// @vitest-environment happy-dom
/**
 * Review round 4 (2026-10-08) and the requirements added with it:
 * - salary lines that used to be misread (18〜25万円, 1万2000円, the kind word far from the amount)
 *   and a salary edit that was reported as "no change";
 * - the last market month read from the data (refreshed monthly), never written in the code;
 * - one market-data cache per screen, and the market chart kept while a new choice loads;
 * - 横断比較 not rebuilt on unrelated state changes;
 * - dummy billing (仮の課金データ（ダミー）) that is always labelled and never added to real billing.
 */
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { EChartProps } from '../../components/EChart';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline, billingText } from './JobTimeline';
import { JobCopyScreen } from './JobCopyScreen';
import { MarketContext } from './MarketContext';
import { MarketCacheContext, marketFetcher } from './marketSource';
import type { MarketCache } from './marketSource';
import { overviewBillingText } from './JobOverview';
import { overviewRange, overviewRow, overviewRows, sortOverview } from './overviewModel';
import { DUMMY_BILLING_LABEL, dummyBillingEntries, withoutRealDays } from './dummyBilling';
import { billingEntries, changeKinds, dummyBilling, periodRows, realBilling, versionChanges } from './timelineModel';
import { canonicalMedia, importBillingCsv } from './billingImport';

const api = vi.hoisted(() => vi.fn());
const spies = vi.hoisted(() => ({ overviewRows: vi.fn() }));
const charts = vi.hoisted(() => ({ mounts: 0 }));
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('../../components/EChart', async () => {
  const { useEffect } = await import('react');
  return { EChart: (props: EChartProps) => {
    useEffect(() => { charts.mounts += 1; }, []);
    return <div data-testid={props.testId}>グラフ</div>;
  } };
});
vi.mock('./overviewModel', async original => {
  const actual = await original<typeof import('./overviewModel')>();
  spies.overviewRows.mockImplementation(actual.overviewRows);
  return { ...actual, overviewRows: spies.overviewRows };
});
afterEach(() => { cleanup(); api.mockReset(); spies.overviewRows.mockClear(); charts.mounts = 0; window.history.replaceState(null, '', '/'); });

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};
const version = (id: string, observedAt: string, body: string): JobCopyRecord['versions'][number] => ({ id, label: id, observedAt, publishedFrom: observedAt, certainty: 'confirmed', kind: 'published', source: '合成', body, applications: null, note: '' });
const salaryJob = (bodies: string[]): JobCopyRecord => ({ ...demo('demo-job-001'), hrhPerformance: undefined,
  versions: bodies.map((body, index) => version(`v${String(index)}`, `2026-09-${String(index * 5 + 1).padStart(2, '0')}T00:00:00Z`, `仕事内容：配送\n給与：${body}\n休日：土日`)) });

describe('a salary edit is never reported as "no change"', () => {
  it('reads 月給18〜25万円 → 月給20〜25万円 as a raise on the salary lane', () => {
    const [, after] = versionChanges(salaryJob(['月給18〜25万円', '月給20〜25万円']));
    expect(after).toMatchObject({ salaryChanged: true, salaryDirection: 'up', otherBodyChanged: false });
    expect(after?.salary).toMatchObject({ kind: '月給', min: 200000, max: 250000 });
    expect(after && changeKinds(after)).toEqual(['給与']);
  });
  it('reads 日給1万5000円 → 日給1万2000円 as a pay cut', () => {
    const [before, after] = versionChanges(salaryJob(['日給1万5000円', '日給1万2000円']));
    expect(before?.salary).toMatchObject({ kind: '日給', min: 15000 });
    expect(after).toMatchObject({ salaryChanged: true, salaryDirection: 'down' });
    expect(after && changeKinds(after)).toEqual(['給与']);
  });
  it('marks a reworded salary line that reads as the same amount as a change of unknown direction', () => {
    const [, after] = versionChanges(salaryJob(['月給25万円', '月給250,000円']));
    expect(after).toMatchObject({ salaryChanged: true, salaryDirection: 'other' });
    expect(after && changeKinds(after)).toEqual(['給与']);
    // The same line (only spacing / full-width digits differ) is not a change.
    const [, same] = versionChanges(salaryJob(['月給25万円', '月給 ２５万円']));
    expect(same).toMatchObject({ salaryChanged: false, salaryDirection: null });
    expect(same && changeKinds(same)).toEqual([]);
  });
  it('marks a change between an unreadable line and a readable one (never "no change")', () => {
    const [, after] = versionChanges(salaryJob(['25万円 ※時給換算1,500円', '月給25万円']));
    expect(after).toMatchObject({ salaryChanged: true, salaryDirection: 'other' });
  });
});

describe('billing CSV media names', () => {
  it('does not take names that only exist on Object.prototype as a media', () => {
    expect(canonicalMedia('constructor')).toBeNull();
    expect(canonicalMedia('Constructor')).toBeNull();
    expect(canonicalMedia('__proto__')).toBeNull();
    expect(canonicalMedia('toString')).toBeNull();
    expect(canonicalMedia('HRハッカー')).toBe('HRハッカー');
    const result = importBillingCsv('媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額\nconstructor,DEMO-SHOP-01,DEMO-HRH-001,2026-09-01,2026-09-30,10000\n', jobs);
    expect(result.periods).toEqual([]);
    expect(result.rejected.map(issue => issue.message).join(' ')).toContain('媒体「constructor」は扱えません');
    expect(result.notFound).toEqual([]);
    expect(JSON.stringify(result)).not.toContain('native code');
  });
});

const series = (months: string[]) => ({ prefecture: '大分県', months, job_count: months.map((_, index) => 100 + index * 10), ctk_count: months.map(() => 300), employer_count: months.map(() => 1), seekers_per_posting: months.map(() => 3) });
const marketData = (path: string, months: string[]) => ({ ok: true, data: { source: '合成', titles: ['ドライバー', '倉庫作業'], prefectures: ['大分県'], ctk_basis: '応募数ではありません', series: path.includes('title=') ? series(months) : null } });

describe('the last market month comes from the data', () => {
  it('shows 市場データは2026年10月まで when the monthly refresh reached October, with no no-data band', async () => {
    api.mockImplementation((path: string) => Promise.resolve(marketData(path, ['2026-06', '2026-07', '2026-08', '2026-09', '2026-10'])));
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('市場データは2026年10月まで（毎月更新）')).toBeTruthy();
    expect(document.querySelector('.jt-nodata')).toBeNull();
    expect(document.body.textContent).not.toContain('2026年8月まで');
    // 2026-09-01〜09-14 and 09-15〜09-24 are inside the data: compared, not "データなし".
    const cells = within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[4]?.textContent);
    expect(cells).toEqual(['同じ月の中（2026年9月 130件）', '同じ月の中（2026年9月 130件）', '+7.7%（2026年9月 130件 → 2026年10月 140件）']);
  });
  it('shows 市場データは2026年8月まで and names the month for a period after it', async () => {
    api.mockImplementation((path: string) => Promise.resolve(marketData(path, ['2026-06', '2026-07', '2026-08'])));
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('市場データは2026年8月まで（毎月更新）。2026年9月以降はデータなしとして表示しています')).toBeTruthy();
    const cells = within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[4]?.textContent);
    expect(cells).toEqual(['データなし（市場求人数は2026年8月まで）', 'データなし（市場求人数は2026年8月まで）', 'データなし（市場求人数は2026年8月まで）']);
  });
  it('shows the last month on the 市場分析 tab too, under the plain label 市場環境', async () => {
    render(<MarketContext job={demo('demo-job-001')} mode="demo" view="table" />);
    fireEvent.change(await screen.findByLabelText('比較する市場職種'), { target: { value: 'ドライバー' } });
    fireEvent.change(screen.getByLabelText('比較する都道府県'), { target: { value: '大分県' } });
    expect(await screen.findByText('市場データは2026年8月まで（毎月更新）')).toBeTruthy();
    expect(screen.getByRole('region', { name: '市場環境' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: /要因/u })).toBeNull();
  });
});

describe('market data cache', () => {
  it('reuses a finished request and asks again after a failure', async () => {
    const cache: MarketCache = new Map();
    api.mockResolvedValueOnce({ ok: false, error: { kind: 'network' } }).mockImplementation((path: string) => Promise.resolve(marketData(path, ['2026-08'])));
    const fetchMarket = marketFetcher('api', cache);
    expect(await fetchMarket('', '')).toEqual({ ok: false });
    expect((await fetchMarket('', '')).ok).toBe(true);
    expect((await fetchMarket('', '')).ok).toBe(true);
    expect(api.mock.calls.map(call => String(call[0]))).toEqual(['/api/job-copy/market', '/api/job-copy/market']);
  });
  it('does not fetch the list and months again when going job 1 → job 2 → job 1 on one screen', async () => {
    api.mockImplementation((path: string) => Promise.resolve(marketData(path, ['2026-08', '2026-09'])));
    const cache: MarketCache = new Map();
    const first = { ...demo('demo-job-001'), title: 'ドライバー' };
    const second = { ...demo('demo-job-002'), title: 'ドライバー', location: '大分県別府市' };
    const view = (job: JobCopyRecord) => <MarketCacheContext.Provider value={cache}><JobTimeline job={job} /></MarketCacheContext.Provider>;
    const { rerender } = render(view(first));
    await screen.findByText(/^市場データは2026年9月まで（毎月更新）/u);
    rerender(view(second));
    await screen.findByText(/^市場データは2026年9月まで（毎月更新）/u);
    rerender(view(first));
    await screen.findByText(/^市場データは2026年9月まで（毎月更新）/u);
    expect(api.mock.calls.map(call => String(call[0]))).toEqual(['/api/job-copy/market', `/api/job-copy/market?${new URLSearchParams({ title: 'ドライバー', prefecture: '大分県' }).toString()}`]);
  });
  it('keeps the market chart mounted while the months of a new occupation load', async () => {
    let release: (value: unknown) => void = () => undefined;
    api.mockImplementation((path: string) => path.includes(encodeURIComponent('倉庫作業'))
      ? new Promise(resolve => { release = resolve; })
      : Promise.resolve(marketData(path, ['2026-08', '2026-09'])));
    render(<JobTimeline job={{ ...demo('demo-job-001'), title: 'ドライバー' }} />);
    await screen.findByText(/^市場データは2026年9月まで（毎月更新）/u);
    expect(charts.mounts).toBe(2);
    fireEvent.change(screen.getByLabelText('職種'), { target: { value: '倉庫作業' } });
    // Still the same chart element while loading, with the loading note shown.
    expect(screen.getByTestId('jt-market')).toBeTruthy();
    expect(within(screen.getByRole('group', { name: '市場' })).getByText('市場データを取得中…')).toBeTruthy();
    await act(async () => { release(marketData('?title=倉庫作業', ['2026-08', '2026-09', '2026-10'])); await Promise.resolve(); });
    await screen.findByText(/^市場データは2026年10月まで（毎月更新）/u);
    expect(screen.getByTestId('jt-market')).toBeTruthy();
    // The applications chart and the market chart were each mounted once: no re-creation.
    expect(charts.mounts).toBe(2);
  });
});

describe('cross-job overview', () => {
  it('does not rebuild the overview when a panel is opened and closed', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    render(<JobCopyScreen />);
    fireEvent.click(screen.getByRole('button', { name: '横断比較' }));
    await screen.findByRole('heading', { name: '求人の横断比較' });
    const calls = spies.overviewRows.mock.calls.length;
    expect(calls).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: 'データ取込' }));
    fireEvent.click(screen.getByRole('button', { name: 'データ取込' }));
    fireEvent.click(screen.getByRole('button', { name: '応募者の条件で探す' }));
    expect(spies.overviewRows.mock.calls.length).toBe(calls);
  });
  it('takes the calendar range from the rows (version days are not diffed again)', () => {
    const job = demo('demo-job-001');
    const row = overviewRow(job, { now: new Date('2026-10-05T03:00:00Z') });
    expect(row.firstDate).toBe('2026-09-01');
    expect(row.changes.map(change => change.to)).toEqual(['2026-09-15', '2026-09-25']);
    // Without the job's application dates, the range still comes from the row's version days.
    expect(overviewRange([], [{ ...row, applicationsAvailable: false }])).toEqual({ start: '2026-09-01', end: '2026-09-25' });
  });
});

describe('dummy billing (仮の課金データ（ダミー）)', () => {
  it('is the same every time for the same job and different for another job', () => {
    const a = dummyBillingEntries('job-a', 'Airワーク', '2026-09-01', '2026-11-30');
    expect(a.map(entry => [entry.start, entry.end, entry.amountYen])).toEqual([['2026-09-01', '2026-09-30', 42000], ['2026-10-01', '2026-10-31', 26000], ['2026-11-01', '2026-11-30', 33000]]);
    expect(dummyBillingEntries('job-a', 'Airワーク', '2026-09-01', '2026-11-30')).toEqual(a);
    expect(dummyBillingEntries('job-b', 'Airワーク', '2026-09-01', '2026-11-30').map(entry => entry.amountYen)).toEqual([48000, 30000, 65000]);
    expect(a.every(entry => entry.source === 'dummy' && entry.plan === DUMMY_BILLING_LABEL)).toBe(true);
  });
  it('gives the days a real row covers to the real row (the dummy keeps only the other days)', () => {
    const dummy = dummyBillingEntries('job-a', 'Airワーク', '2026-09-01', '2026-09-30');
    const pieces = withoutRealDays(dummy, [{ source: 'hrhacker', start: '2026-09-10', end: '2026-09-19', amountYen: 50000 }]);
    expect(pieces.map(entry => [entry.start, entry.end, entry.amountYen])).toEqual([['2026-09-01', '2026-09-09', 12600], ['2026-09-20', '2026-09-30', 15400]]);
    // demo-job-001 has HRハッカー実績 for every day from the first version to the counts day: no dummy at all.
    const real = billingEntries(demo('demo-job-001'));
    expect(dummyBilling(real)).toEqual([]);
    expect(realBilling(real).map(entry => entry.amountYen)).toEqual([30000, 45000, 12000]);
  });
  it('never adds the dummy into a total: the period table and the overview show the real amount only', () => {
    const job: JobCopyRecord = { ...demo('demo-job-001'), id: 'job-a',
      overallApplications: { total: 0, missingDate: 0, fetchedAt: '2026-09-30T09:00:00+09:00', distributions: {}, byDate: {} },
      hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: 'DEMO-HRH-001', captured_at: '2026-09-30T00:00:00Z', rows: [{ period_start: '2026-09-10', period_end: '2026-09-19', impressions: null, clicks: null, cost_yen: 50000, applications: null }] },
      versions: [version('v1', '2026-09-01T00:00:00Z', '給与：月給25万円')] };
    const [row] = periodRows(job, { asOf: '2026-09-30' });
    expect(row?.billing).toMatchObject({ connected: true, yen: 50000 });
    expect(row?.dummyBilling).toBe(true);
    expect(row && billingText(row)).toBe(`5万円（${DUMMY_BILLING_LABEL}は合計に入れていません）`);
    const overview = overviewRow(job, { now: new Date('2026-09-30T03:00:00Z') });
    expect(overview.billingYen).toBe(50000);
    expect(overview.hasDummyBilling).toBe(true);
    expect(overviewBillingText(overview)).toBe(`5万円（${DUMMY_BILLING_LABEL}は合計に入れていません）`);
    // Neither 7万8,000円 (the sum) nor the dummy 2万8,000円 is shown as a total.
    expect(billingText(row ?? periodRows(job, { asOf: '2026-09-30' })[0] as never)).not.toMatch(/7万8,000円|2万8,000円/u);
    // With the switch off, only the real amount is left.
    expect(periodRows(job, { asOf: '2026-09-30', dummyBilling: false })[0]?.dummyBilling).toBe(false);
    expect(overviewBillingText(overviewRow(job, { now: new Date('2026-09-30T03:00:00Z'), dummyBilling: false }))).toBe('5万円');
  });
  it('labels the dummy in the 課金 lane and keeps it out of the period table, the overview totals and the sort', () => {
    render(<JobTimeline job={demo('demo-job-002')} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} />);
    const lane = screen.getByRole('group', { name: '課金' });
    expect(lane.textContent).toContain(DUMMY_BILLING_LABEL);
    expect(lane.querySelectorAll('.jt-billing-dummy')).toHaveLength(2);
    expect([...lane.querySelectorAll('.jt-billing-dummy')].every(bar => (bar.getAttribute('title') ?? '').startsWith(DUMMY_BILLING_LABEL))).toBe(true);
    expect(screen.getByText(/仮の課金データ（ダミー）は、実際の課金データがまだ無いため表示している架空の金額です/u)).toBeTruthy();
    const billingCells = within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[3]?.textContent);
    expect(billingCells).toEqual([`実際の課金データなし（${DUMMY_BILLING_LABEL}は合計しません）`, `実際の課金データなし（${DUMMY_BILLING_LABEL}は合計しません）`]);
    expect(within(screen.getByRole('table')).queryByText(/3万8,000円|3万4,802円/u)).toBeNull();
    const rows = overviewRows(jobs);
    expect(rows.map(row => [row.jobId, row.billingYen, row.hasDummyBilling])).toEqual([
      ['demo-job-001', 87000, false], ['demo-job-002', null, true], ['demo-job-003', 48000, false], ['demo-job-004', null, true],
      ['demo-job-005', null, true], ['demo-job-006', null, true], ['demo-job-007', null, true], ['demo-job-008', null, false],
    ]);
    expect(rows.map(overviewBillingText).filter(text => text.includes('ダミー'))).toEqual(Array.from({ length: 5 }, () => `実際の課金データなし（${DUMMY_BILLING_LABEL}は合計しません）`));
    // Sorting by billing uses real totals only; jobs with only the dummy have no value and keep their order after them.
    expect(sortOverview(rows, 'billing').map(row => row.jobId)).toEqual(['demo-job-001', 'demo-job-003', 'demo-job-002', 'demo-job-004', 'demo-job-005', 'demo-job-006', 'demo-job-007', 'demo-job-008']);
  });
});
