// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { EChartProps } from '../../components/EChart';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { JobCopyScreen } from './JobCopyScreen';
import { JobOverview } from './JobOverview';

const charts = vi.hoisted(() => ({ props: [] as EChartProps[] }));
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
// These tests check the screen without the dummy billing (the state once real billing is
// connected and DUMMY_BILLING_ENABLED is set to false). timelineRound4.test.tsx covers the dummy.
vi.mock('./dummyBilling', async (original) => ({ ...await original<typeof import('./dummyBilling')>(), DUMMY_BILLING_ENABLED: false }));
vi.mock('../../components/EChart', () => ({ EChart: (props: EChartProps) => { charts.props.push(props); return <div data-testid={props.testId}>グラフ</div>; } }));
afterEach(() => { cleanup(); vi.resetAllMocks(); charts.props = []; });

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};
const forbidden = ['確実に', '必ず', '効果', '100%', '観測版', '本文観測', '観測ラベル', '版対応不明', 'ctk', 'MOC', 'snapshot', 'fixture'];
const lastChart = (testId: string) => charts.props.filter(props => props.testId === testId).at(-1);
type Series = { name: string; data: [number, number | null][] }[];

describe('job timeline lanes', () => {
  it('shows the seven lanes, the period table and no causal or developer wording', async () => {
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    const lanes = screen.getAllByRole('group').map(group => group.getAttribute('aria-label'));
    expect(lanes.filter(name => ['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場'].includes(name ?? ''))).toEqual(['掲載期間', '給与', '本文', '画像', '課金', '応募', '市場']);
    await screen.findByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください');
    const table = screen.getByRole('table');
    const rows = within(table).getAllByRole('row').slice(1).map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent));
    // The demo's HRハッカー amounts are made up, and say so in the table (the banner is not printed).
    expect(rows[0]?.slice(0, 4)).toEqual(['14日', '7件', '0.50件/日', 'デモ用の架空の金額 3万円']);
    expect(rows[1]?.slice(0, 4)).toEqual(['10日', '8件', '0.80件/日', 'デモ用の架空の金額 約2万8,125円']);
    expect(rows[2]?.slice(0, 3)).toEqual(['11日', '3件', '0.27件/日']);
    const text = document.body.textContent;
    for (const word of forbidden) expect(text).not.toContain(word);
    expect(text).toContain('応募は HubSpot に記録されたものだけです');
  });

  it('says 課金データなし without billing data, says it is not 0円, and never shows 0円 as an amount', () => {
    render(<JobTimeline job={demo('demo-job-005')} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '課金' });
    // The explanation is visible text (not a hover-only title).
    expect(lane.textContent).toContain('課金データなし（0円という意味ではありません）');
    expect(lane.querySelector('[title]')).toBeNull();
    expect(document.body.textContent.replace('0円という意味ではありません', '')).not.toMatch(/(^|[^\d,])0円/);
    expect(within(screen.getByRole('table')).getAllByText('課金データなし').length).toBe(2);
    expect(document.body.textContent).not.toContain('未接続');
  });

  it('marks browser-only CSV billing as lost on reload', () => {
    render(<JobTimeline job={demo('demo-job-005')} marketMode="demo" billing={[{ source: 'csv', start: '2026-09-12', end: '2026-09-25', amountYen: 18000, taxIncluded: true }]} />);
    expect(screen.getByRole('note').textContent).toContain('再読み込みすると消えます');
    expect(screen.getByRole('group', { name: '課金' }).textContent).toContain('1万8,000円');
  });

  it('draws no market value for 2026-09 when the data ends at 2026-08 and labels the gap', async () => {
    const months = ['2026-06', '2026-07', '2026-08'];
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: { source: '合成', titles: ['ドライバー'], prefectures: ['大分県'], ctk_basis: '応募数ではありません',
      series: path.includes('title=') ? { prefecture: '大分県', months, job_count: [90, 100, 110], ctk_count: [300, 310, 320], employer_count: [1, 1, 1], seekers_per_posting: [3, 3, 3] } : null } }));
    const job: JobCopyRecord = { ...demo('demo-job-001'), overallApplications: { total: 3, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, byDate: { '2026-08-10': 1, '2026-09-02': 2 } } };
    render(<JobTimeline job={job} />);
    await waitFor(() => { expect(lastChart('jt-market')).toBeDefined(); });
    expect(api.mock.calls.map(call => String(call[0]))).toEqual(['/api/job-copy/market', '/api/job-copy/market?title=%E3%83%89%E3%83%A9%E3%82%A4%E3%83%90%E3%83%BC&prefecture=%E5%A4%A7%E5%88%86%E7%9C%8C']);
    const series = (lastChart('jt-market')?.option as { series: Series }).series;
    const jobsSeries = series.find(item => item.name === '市場求人数');
    expect(jobsSeries?.data.map(point => point[1])).toEqual([110, null, null]);
    expect(series.find(item => item.name === 'Indeed閲覧者指標')?.data.map(point => point[1])).toEqual([320, null, null]);
    expect(screen.getByText('市場データは2026年8月まで（毎月更新）。2026年9月以降はデータなしとして表示しています')).toBeTruthy();
    expect(screen.getByLabelText<HTMLSelectElement>('職種').value).toBe('ドライバー');
    expect(screen.getByLabelText<HTMLSelectElement>('都道府県').value).toBe('大分県');
    // The first period is all in 2026-09, after the data ends: say up to which month there is data.
    expect(within(screen.getByRole('table')).getAllByRole('row')[1]?.querySelectorAll('td')[4]?.textContent).toBe('データなし（市場求人数は2026年8月まで）');
  });

  it('keeps the months with data when a period runs past 2026-08 and writes months as 2026年7月', async () => {
    const months = ['2026-06', '2026-07', '2026-08'];
    api.mockImplementation((path: string) => Promise.resolve({ ok: true, data: { source: '合成', titles: ['ドライバー'], prefectures: ['大分県'], ctk_basis: '応募数ではありません',
      series: path.includes('title=') ? { prefecture: '大分県', months, job_count: [90, 100, 110], ctk_count: [300, 310, 320], employer_count: [1, 1, 1], seekers_per_posting: [3, 3, 3] } : null } }));
    const base = demo('demo-job-001');
    const last = base.versions.find(version => version.id === 'demo-001-v3');
    if (!last) throw new Error('Missing demo-001-v3');
    const job: JobCopyRecord = { ...base, hrhPerformance: undefined, versions: [{ ...last, publishedFrom: '2026-07-01T10:00:00+09:00', observedAt: '2026-07-01T10:00:00+09:00' }] };
    render(<JobTimeline job={job} />);
    await waitFor(() => { expect(lastChart('jt-market')).toBeDefined(); });
    const cells = within(screen.getByRole('table')).getAllByRole('row')[1]?.querySelectorAll('td');
    expect(cells?.[4]?.textContent).toBe('+10.0%（2026年7月 100件 → 2026年8月 110件、2026年9月以降はデータなし）');
    expect(document.body.textContent).not.toMatch(/\d{4}-\d{2}(?!-)/);
    expect(document.body.textContent).not.toContain('年09月');
  });

  it('says there is no market data for the choice when the series is empty (not "from the range start")', async () => {
    api.mockImplementation(() => Promise.resolve({ ok: true, data: { source: '合成', titles: ['ドライバー'], prefectures: ['大分県'], ctk_basis: '応募数ではありません', series: null } }));
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('この職種・都道府県の市場データはありません')).toBeTruthy();
    expect(document.body.textContent).not.toContain('以降は市場データがありません');
    expect(lastChart('jt-market')).toBeUndefined();
  });

  it('says how the prefecture was chosen, separately from the occupation', async () => {
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    await screen.findByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください');
    expect(screen.getByText('勤務地から大分県を自動で選びました')).toBeTruthy();
    const other = [...screen.getByLabelText<HTMLSelectElement>('都道府県').options].map(option => option.value).find(value => value && value !== '大分県');
    if (!other) throw new Error('No other prefecture in the demo market data');
    fireEvent.change(screen.getByLabelText('都道府県'), { target: { value: other } });
    expect(await screen.findByText('手で選んだ都道府県です')).toBeTruthy();
    // The occupation was not touched, so its note stays.
    expect(screen.getByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください')).toBeTruthy();
    expect(screen.queryByText('手で選んだ職種です')).toBeNull();
  });

  it('shows 未取得 (not 0件) in the period table when application dates were never fetched', () => {
    const job: JobCopyRecord = { ...demo('demo-job-001') };
    delete job.overallApplications;
    render(<JobTimeline job={job} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} />);
    const rows = within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent));
    expect(rows.map(row => row.slice(1, 3))).toEqual([['未取得', '未取得'], ['未取得', '未取得'], ['未取得', '未取得']]);
    expect(screen.getByRole('table').textContent).not.toMatch(/0件|0\.00件\/日/);
  });

  it('asks for a category instead of guessing when the title matches none', async () => {
    render(<JobTimeline job={demo('demo-job-002')} marketMode="demo" />);
    expect(await screen.findByText('求人名から職種を決められませんでした')).toBeTruthy();
    expect(screen.getByLabelText<HTMLSelectElement>('職種').value).toBe('');
    expect(screen.getByLabelText<HTMLSelectElement>('都道府県').value).toBe('大分県');
    expect(lastChart('jt-market')).toBeUndefined();
    fireEvent.change(screen.getByLabelText('職種'), { target: { value: '倉庫作業' } });
    expect(await screen.findByText('手で選んだ職種です')).toBeTruthy();
    await waitFor(() => { expect(lastChart('jt-market')).toBeDefined(); });
  });

  it('switches application bars between day, week and month and leaves undated ones out', () => {
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    const total = () => ((lastChart('jt-applications')?.option as { series: { data: { value: [number, number] }[] }[] }).series[0]?.data ?? []).reduce((sum, item) => sum + item.value[1], 0);
    expect(total()).toBe(18);
    fireEvent.click(screen.getByRole('button', { name: '日ごと' }));
    expect((lastChart('jt-applications')?.option as { series: { data: unknown[] }[] }).series[0]?.data).toHaveLength(12);
    fireEvent.click(screen.getByRole('button', { name: '月ごと' }));
    expect(total()).toBe(18);
    expect(screen.getByText('応募日が分からない応募 10件 はグラフに含めていません')).toBeTruthy();
  });

  it('opens the body or the diff from a selected version', () => {
    const open = vi.fn(); const compare = vi.fn();
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" onOpenVersion={open} onCompareVersions={compare} />);
    fireEvent.click(screen.getByRole('button', { name: /給与・勤務条件変更の本文：\d+行追加・\d+行削除/ }));
    // The selected period row says so in text and with aria-current, not only by colour.
    const selectedRow = screen.getByRole('table').querySelector('tr[aria-current="true"]');
    expect(selectedRow?.textContent).toContain('選択中');
    expect(selectedRow?.textContent).toContain('給与・勤務条件変更');
    fireEvent.click(screen.getByRole('button', { name: '本文・画像を開く' }));
    expect(open).toHaveBeenCalledWith('demo-001-v2');
    fireEvent.click(screen.getByRole('button', { name: '前の版との差分を開く' }));
    expect(compare).toHaveBeenCalledWith('demo-001-v1', 'demo-001-v2');
  });
});

describe('job copy screen integration (demo mode)', () => {
  it('opens on the timeline tab and switches the list to the cross-job overview', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    render(<JobCopyScreen />);
    const primary = screen.getByRole('tablist', { name: '求人管理の機能' });
    expect(within(primary).getAllByRole('tab')[0]?.textContent).toBe('タイムライン');
    expect(within(primary).getByRole('tab', { name: 'タイムライン' }).getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('group', { name: '応募' })).toBeTruthy();
    await screen.findByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください');
    expect(api).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '横断比較' }));
    const table = screen.getByRole('region', { name: '求人の横断比較の表' });
    const first = within(table).getAllByRole('row')[1];
    expect(first?.textContent).toContain('地域配送ドライバー');
    expect(first?.textContent).toContain('2026/09/25');
    expect(first?.textContent).toContain('0.80件/日');
    expect(first?.textContent).toContain('8件 / 10日');
    expect(first?.textContent).toContain('0.27件/日');
    expect(first?.textContent).toContain('8万7,000円');
    fireEvent.click(within(table).getByRole('button', { name: '受付事務スタッフ' }));
    expect(screen.getByRole('heading', { level: 1, name: '受付事務スタッフ' })).toBeTruthy();
  });
});

describe('market cells of the period table while loading or after a failure (review round 2)', () => {
  const marketCells = () => within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[4]?.textContent);
  const list = { source: '合成', titles: ['ドライバー', '倉庫作業'], prefectures: ['大分県', '福岡県'], ctk_basis: '応募数ではありません', series: null };
  const series = { prefecture: '大分県', months: ['2026-08', '2026-09', '2026-10'], job_count: [100, 110, 121], ctk_count: [300, 310, 320], employer_count: [1, 1, 1], seekers_per_posting: [3, 3, 3] };

  it('says 取得できませんでした (not "pick a market") when the market list request fails', async () => {
    api.mockResolvedValue({ ok: false, error: { message: '500' } });
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('市場データを取得できませんでした')).toBeTruthy();
    expect(marketCells()).toEqual(['取得できませんでした', '取得できませんでした', '取得できませんでした']);
  });

  it('says 取得できませんでした (not データなし) when the request for the chosen market fails', async () => {
    api.mockImplementation((path: string) => Promise.resolve(path.includes('title=') ? { ok: false, error: { message: 'timeout' } } : { ok: true, data: list }));
    render(<JobTimeline job={demo('demo-job-001')} />);
    expect(await screen.findByText('市場データを取得できませんでした')).toBeTruthy();
    expect(marketCells()).toEqual(['取得できませんでした', '取得できませんでした', '取得できませんでした']);
    expect(screen.getByRole('table').textContent).not.toContain('データなし');
  });

  it('says 取得中… while the chosen market is loading', async () => {
    api.mockImplementation((path: string) => path.includes('title=') ? new Promise(() => undefined) : Promise.resolve({ ok: true, data: list }));
    render(<JobTimeline job={demo('demo-job-001')} />);
    await screen.findByText('求人名に含まれる職種を自動で選びました。違う場合は選び直してください');
    expect(screen.getByText('市場データを取得中…')).toBeTruthy();
    expect(marketCells()).toEqual(['取得中…', '取得中…', '取得中…']);
    expect(screen.getByRole('table').textContent).not.toContain('市場を選ぶと表示');
  });

  it('keeps a hand-picked prefecture when the market data is fetched again after a failure', async () => {
    let seriesCalls = 0;
    api.mockImplementation((path: string) => {
      if (!path.includes('title=')) return Promise.resolve({ ok: true, data: list });
      seriesCalls += 1;
      return Promise.resolve(path.includes(encodeURIComponent('福岡県')) && seriesCalls === 2 ? { ok: false, error: { message: '500' } } : { ok: true, data: { ...list, series } });
    });
    render(<JobTimeline job={demo('demo-job-001')} />);
    await waitFor(() => { expect(lastChart('jt-market')).toBeDefined(); });
    fireEvent.change(screen.getByLabelText('都道府県'), { target: { value: '福岡県' } });
    expect(await screen.findByRole('button', { name: '市場データを再取得' })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '市場データを再取得' }));
    await waitFor(() => { expect(seriesCalls).toBe(3); });
    await waitFor(() => { expect(screen.queryByRole('button', { name: '市場データを再取得' })).toBeNull(); });
    expect(screen.getByLabelText<HTMLSelectElement>('都道府県').value).toBe('福岡県');
    expect(screen.getByText('手で選んだ都道府県です')).toBeTruthy();
    const paths = api.mock.calls.map(call => String(call[0]));
    expect(paths.filter(path => !path.includes('title='))).toHaveLength(1);
    expect(paths.at(-1)).toContain(encodeURIComponent('福岡県'));
  });
});

describe('period table rows without counts or periods (review round 2)', () => {
  it('shows 応募集計の取得後に始まった期間 (not 0日 / 0件) for a period that starts after the counts were taken', () => {
    const base = demo('demo-job-001');
    if (!base.overallApplications) throw new Error('Missing counts');
    const job: JobCopyRecord = { ...base, overallApplications: { ...base.overallApplications, fetchedAt: '2026-09-20T09:00:00+09:00' } };
    render(<JobTimeline job={job} marketMode="demo" />);
    const rows = within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent));
    expect(rows.at(-1)?.slice(0, 3)).toEqual(['—', '応募集計の取得後に始まった期間', '—']);
    expect(rows[0]?.slice(0, 3)).toEqual(['14日', '7件', '0.50件/日']);
  });

  it('explains instead of showing an empty table when no posting period is known', () => {
    const job: JobCopyRecord = { ...demo('demo-job-001'), versions: [], hrhPerformance: undefined, overallApplications: { total: 2, missingDate: 0, fetchedAt: '2026-10-05T00:00:00Z', distributions: {}, byDate: { '2026-09-01': 2 } } };
    render(<JobTimeline job={job} marketMode="demo" />);
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getByText('掲載期間が取得できていないため、期間ごとの比較はできません。')).toBeTruthy();
  });
});

describe('cross-job overview billing cell', () => {
  it('shows 期間が重なる課金あり instead of a sum when CSV billing overlaps HRハッカー billing', () => {
    render(<JobOverview records={[demo('demo-job-001')]} billing={{ 'demo-job-001': [{ source: 'csv', start: '2026-09-10', end: '2026-09-20', amountYen: 5000, media: 'HRハッカー' }] }} onChoose={() => undefined} />);
    const row = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getAllByRole('row')[1];
    expect(row?.textContent).toContain('期間が重なる課金あり');
    expect(row?.textContent).not.toContain('8万7,000円');
    expect(row?.textContent).not.toContain('9万2,000円');
  });
});
