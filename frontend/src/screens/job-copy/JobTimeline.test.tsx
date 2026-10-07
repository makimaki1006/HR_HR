// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { EChartProps } from '../../components/EChart';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { JobCopyScreen } from './JobCopyScreen';

const charts = vi.hoisted(() => ({ props: [] as EChartProps[] }));
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
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
    expect(rows[0]?.slice(0, 4)).toEqual(['14日', '7件', '0.50件/日', '3万円']);
    expect(rows[1]?.slice(0, 4)).toEqual(['10日', '8件', '0.80件/日', '約2.8万円']);
    expect(rows[2]?.slice(0, 3)).toEqual(['11日', '3件', '0.27件/日']);
    const text = document.body.textContent;
    for (const word of forbidden) expect(text).not.toContain(word);
    expect(text).toContain('応募は HubSpot に記録されたものだけです');
  });

  it('says 未接続 without billing data and never 0円', () => {
    render(<JobTimeline job={demo('demo-job-005')} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '課金' });
    expect(lane.textContent).toContain('未接続');
    expect(document.body.textContent).not.toMatch(/(^|[^\d,])0円/);
    expect(within(screen.getByRole('table')).getAllByText('未接続').length).toBe(2);
  });

  it('marks browser-only CSV billing as lost on reload', () => {
    render(<JobTimeline job={demo('demo-job-005')} marketMode="demo" billing={[{ source: 'csv', start: '2026-09-12', end: '2026-09-25', amountYen: 18000, taxIncluded: true }]} />);
    expect(screen.getByRole('note').textContent).toContain('再読み込みすると消えます');
    expect(screen.getByRole('group', { name: '課金' }).textContent).toContain('1.8万円');
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
    expect(screen.getByText(/2026年09月以降は市場データがありません（2026年08月まで）/)).toBeTruthy();
    expect(screen.getByLabelText<HTMLSelectElement>('職種').value).toBe('ドライバー');
    expect(screen.getByLabelText<HTMLSelectElement>('都道府県').value).toBe('大分県');
    // The first period is all in 2026-09, after the data ends.
    expect(within(screen.getByRole('table')).getAllByRole('row')[1]?.querySelectorAll('td')[4]?.textContent).toBe('データなし');
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
    fireEvent.click(screen.getByRole('button', { name: /給与・勤務条件変更の本文：追加\d+行・削除\d+行/ }));
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
    expect(first?.textContent).toContain('0.64件/日');
    expect(first?.textContent).toContain('0.27件/日');
    expect(first?.textContent).toContain('8.7万円');
    fireEvent.click(within(table).getByRole('button', { name: '受付事務スタッフ' }));
    expect(screen.getByRole('heading', { level: 1, name: '受付事務スタッフ' })).toBeTruthy();
  });
});
