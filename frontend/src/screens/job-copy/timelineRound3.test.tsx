// @vitest-environment happy-dom
/**
 * Review round 3 (2026-10-08): salary direction, selection panel, retry focus, live region,
 * keyboard-reachable explanations, the top-bar count, short overview windows, demo market data in
 * 市場分析, YYYY/MM months and plain wording in the diff panel and the header.
 */
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EChartProps } from '../../components/EChart';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { MarketContext } from './MarketContext';
import { ApplicationTrend } from './ApplicationTrend';
import { JobCopyScreen } from './JobCopyScreen';
import { versionChanges } from './timelineModel';
import { applicationsOutsideTimeline } from './applicationCountsModel';
import { MIN_RATE_DAYS, overviewRows, sortOverview } from './overviewModel';
import { parseMediaCapture } from './mediaCaptureParser';

const charts = vi.hoisted(() => ({ props: [] as EChartProps[] }));
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
// These tests check the screen without the dummy billing (the state once real billing is
// connected and DUMMY_BILLING_ENABLED is set to false). timelineRound4.test.tsx covers the dummy.
vi.mock('./dummyBilling', async (original) => ({ ...await original<typeof import('./dummyBilling')>(), DUMMY_BILLING_ENABLED: false }));
vi.mock('../../components/EChart', () => ({ EChart: (props: EChartProps) => { charts.props.push(props); return <div data-testid={props.testId}>グラフ</div>; } }));
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.restoreAllMocks(); charts.props = []; });

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};
const version = (id: string, observedAt: string, body: string): JobCopyRecord['versions'][number] => ({ id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '' });
const synthetic = (versions: JobCopyRecord['versions'], extra: Partial<JobCopyRecord> = {}): JobCopyRecord => ({ id: 'synthetic', title: '合成ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: 'S-1', location: '大分県大分市', versions, ...extra });

describe('salary direction (▲ only for a raise)', () => {
  it('marks a raise ▲ and a cut ▼ on the demo driver job (25万〜28万 → 27万〜30万 → 25万〜28万)', () => {
    expect(versionChanges(demo('demo-job-001')).map(change => change.salaryDirection)).toEqual([null, 'up', 'down']);
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    const labels = [...screen.getByRole('group', { name: '給与' }).querySelectorAll('.jt-salary-label')].map(label => label.textContent);
    expect(labels).toEqual(['月給25万〜28万円', '▲月給27万〜30万円', '▼月給25万〜28万円']);
    expect(screen.getByRole('img', { name: '前の版より下がった' }).textContent).toBe('▼');
    expect(screen.getByRole('img', { name: '前の版より上がった' }).textContent).toBe('▲');
  });
  it('says 変更 (not ▲ or ▼) when the kind of pay changes', () => {
    const job = synthetic([version('a', '2026-07-01T00:00:00Z', '給与：月給250,000円'), version('b', '2026-08-01T00:00:00Z', '給与：時給1,500円')]);
    expect(versionChanges(job).map(change => change.salaryDirection)).toEqual([null, 'other']);
    render(<JobTimeline job={job} marketMode="demo" />);
    expect(screen.getByRole('img', { name: '前の版から変わった' }).textContent).toBe('変更');
  });
  it('compares the upper bound when the lower bound is the same', () => {
    const job = synthetic([version('a', '2026-07-01T00:00:00Z', '給与：月給250,000円〜300,000円'), version('b', '2026-08-01T00:00:00Z', '給与：月給250,000円〜280,000円')]);
    expect(versionChanges(job).map(change => change.salaryDirection)).toEqual([null, 'down']);
  });
});

describe('selection, retry focus and announcements', () => {
  it('scrolls the 選んだ版 panel into view after a mark is chosen, and the panel sits right under the lanes', () => {
    const scroll = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, writable: true, value: scroll });
    render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" onOpenVersion={() => undefined} onCompareVersions={() => undefined} />);
    fireEvent.click(screen.getByRole('button', { name: /給与・勤務条件変更の本文/ }));
    const panel = screen.getByRole('region', { name: '選んだ版' });
    expect(scroll).toHaveBeenCalledWith({ block: 'nearest' });
    expect(scroll.mock.contexts.at(-1)).toBe(panel);
    expect(panel.previousElementSibling?.classList.contains('jt-lanes')).toBe(true);
    expect(within(panel).getByRole('button', { name: '前の版との差分を開く' })).toBeTruthy();
  });

  it('keeps the retry button while retrying, then moves focus to 職種 and announces the result', async () => {
    const list = { source: '合成', titles: ['ドライバー'], prefectures: ['大分県'], ctk_basis: '応募数ではありません', series: null };
    const series = { prefecture: '大分県', months: ['2026-08', '2026-09'], job_count: [100, 110], ctk_count: [300, 310], employer_count: [1, 1], seekers_per_posting: [3, 3] };
    let release: (value: unknown) => void = () => undefined;
    let calls = 0;
    api.mockImplementation((path: string) => {
      if (!path.includes('title=')) return Promise.resolve({ ok: true, data: list });
      calls += 1;
      if (calls === 1) return Promise.resolve({ ok: false, error: { message: '500' } });
      return new Promise(resolve => { release = resolve; });
    });
    render(<JobTimeline job={demo('demo-job-001')} />);
    const status = () => screen.getByRole('group', { name: '市場' }).parentElement?.querySelector('[role="status"]')?.textContent;
    const retry = await screen.findByRole('button', { name: '市場データを再取得' });
    expect(status()).toBe('市場データの取得に失敗しました。「市場データを再取得」で取り直せます');
    retry.focus();
    fireEvent.click(retry);
    // Still on screen (aria-disabled) while loading, so focus stays on it.
    expect(screen.getByRole('button', { name: '市場データを再取得' }).getAttribute('aria-disabled')).toBe('true');
    expect(document.activeElement).toBe(screen.getByRole('button', { name: '市場データを再取得' }));
    expect(status()).toBe('市場データを取り直しています');
    await act(async () => { release({ ok: true, data: { ...list, series } }); await Promise.resolve(); });
    await waitFor(() => { expect(screen.queryByRole('button', { name: '市場データを再取得' })).toBeNull(); });
    expect(document.activeElement).toBe(screen.getByLabelText('職種'));
    expect(status()).toBe('市場データを表示しました（ドライバー・大分県）');
  });
});

describe('explanations reachable without hovering', () => {
  it('puts the timeline caveats behind one keyboard-reachable ⓘ and keeps one HubSpot-only line', () => {
    const { container } = render(<JobTimeline job={demo('demo-job-001')} marketMode="demo" />);
    const timeline = screen.getByRole('region', { name: 'タイムライン' });
    expect(timeline.querySelectorAll('span[title]')).toHaveLength(0);
    const summaries = [...timeline.querySelectorAll('details.jc-infotip > summary')].map(node => node.textContent);
    expect(summaries).toEqual(['並べて見るための表示です ⓘ', 'Indeed閲覧者指標 ⓘ']);
    const tip = timeline.querySelector('details.jc-infotip');
    expect(tip?.textContent).toContain('応募が増えた・減った理由を示すものではありません');
    expect(tip?.textContent).toContain('「1日あたり」で並べて確認してください');
    // HubSpot-only is said once on screen (the ⓘ body is hidden until opened).
    const visible = container.textContent.replace(tip?.querySelector('.jc-infotip-body')?.textContent ?? '', '');
    expect(visible.match(/HubSpot に記録された/gu)).toHaveLength(1);
    expect(visible).not.toContain('版が切り替わった日で期間を区切っています');
    const legend = screen.getByRole('group', { name: '凡例' });
    // The demo has media publication times; the legend says so (no 確定/推定/不明 scale).
    expect(legend.textContent).toBe('掲載日：媒体の掲載日時');
  });
});

describe('top-bar count matches the period table', () => {
  it('counts undated applications, applications before the first acquisition and between acquisitions (not every application)', () => {
    const job = synthetic([version('a', '2026-07-01T00:00:00Z', '給与：月給230,000円'), version('b', '2026-08-20T00:00:00Z', '給与：月給250,000円')], {
      dataSource: 'hubspot',
      overallApplications: { total: 9, missingDate: 1, fetchedAt: '2026-08-20T00:00:00Z', distributions: {}, byDate: { '2026-06-20': 2, '2026-07-10': 2, '2026-07-25': 1, '2026-08-20': 3 } },
    });
    // 1 undated + 2 before the first acquisition (06-20) + 3 between the two acquisitions (07-10, 07-25:
    // the salary changed somewhere between 07-01 and 08-20) = 6; the other 3 are in version rows (08-20).
    expect(applicationsOutsideTimeline(job)).toBe(6);
    expect(applicationsOutsideTimeline(synthetic([]))).toBeNull();
  });
});

describe('cross-job overview', () => {
  it(`does not compare or sort by a rate from fewer than ${String(MIN_RATE_DAYS)} days`, () => {
    const job = synthetic([version('a', '2026-08-01T00:00:00Z', '給与：月給230,000円'), version('b', '2026-08-20T00:00:00Z', '給与：月給250,000円')], {
      dataSource: 'hubspot', overallApplications: { total: 4, missingDate: 0, fetchedAt: '2026-08-20T00:00:00Z', distributions: {}, byDate: { '2026-08-10': 2, '2026-08-20': 2 } },
    });
    const longer = { ...demo('demo-job-001') };
    const rows = overviewRows([job, longer]);
    expect(rows[0]?.after).toMatchObject({ days: 1, applications: 2 });
    expect(sortOverview(rows, 'afterPerDay').map(row => row.jobId)).toEqual(['demo-job-001', 'synthetic']);
    render(<JobOverview records={[job]} onChoose={() => undefined} />);
    const row = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getAllByRole('row')[1];
    expect(row?.textContent).toContain('期間が短いため比べません2件 / 1日');
    expect(row?.textContent).not.toContain('2.00件/日');
    expect(row?.textContent).toContain('課金データなし');
    expect(row?.textContent).not.toContain('未接続');
  });
});

describe('市場分析 in demo mode and month labels', () => {
  it('uses the fictional market data in demo mode and never calls the API', async () => {
    render(<MarketContext job={demo('demo-job-001')} mode="demo" view="table" />);
    expect(await screen.findByText(/架空の市場データ（デモ）/u)).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(api).not.toHaveBeenCalled();
  });
  it('styles the retry button as a button', async () => {
    api.mockResolvedValue({ ok: false });
    render(<MarketContext job={demo('demo-job-001')} />);
    const button = await screen.findByRole('button', { name: '市場データを再取得' });
    expect(button.className).toBe('jc-button');
    expect(button.getAttribute('type')).toBe('button');
  });
  it('writes months as YYYY/MM on the monthly applications chart and table', () => {
    render(<ApplicationTrend job={demo('demo-job-001')} />);
    const option = charts.props.find(props => props.testId === 'jc-applications-monthly')?.option as { xAxis: { data: string[] } } | undefined;
    expect(option?.xAxis.data.every(month => /^\d{4}\/\d{2}$/u.test(month))).toBe(true);
    const heads = [...document.querySelectorAll('tbody th')].map(cell => cell.textContent);
    expect(heads.length).toBeGreaterThan(0);
    expect(heads.every(text => /^\d{4}\/\d{2}$/u.test(text))).toBe(true);
  });
});

describe('plain wording for versions, the header and the diff panel', () => {
  it('names captured versions by date', () => {
    const capture = { schemaVersion: 1, capturedAt: '2026-08-20T00:00:00Z', jobs: [{ id: 'j', title: '合成', company: '合成', media: 'HRハッカー', mediaJobId: '1', location: '大分県', body: '本文', images: [],
      history: [{ id: 'p', capturedAt: '2026-07-01T00:00:00Z', body: '前の本文', images: [] }] }] };
    expect(parseMediaCapture(JSON.stringify(capture))[0]?.versions.map(item => item.label)).toEqual(['2026/07/01時点の求人内容', '2026/08/20時点の求人内容']);
  });

  beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); });
  it('shows plain header wording and one image notice in the diff panel', () => {
    render(<JobCopyScreen />);
    const meta = document.querySelector('.jc-record-meta');
    expect(meta?.textContent).toContain('表示中の文面: 初回文面への復帰');
    expect(meta?.textContent).toContain('HubSpotの求人と未連携');
    expect(meta?.textContent).not.toMatch(/接続待ち|実求人ID|取得した版/u);
    fireEvent.click(screen.getByRole('tab', { name: '比較・報告' }));
    const diffTab = screen.queryByRole('tab', { name: '変更差分' });
    if (diffTab) fireEvent.click(diffTab);
    const overview = screen.getByRole('region', { name: '比較結果の要約' });
    expect(overview.textContent).toContain('画像の差し替え・並び順');
    expect(overview.textContent).toContain('画像の中身');
    expect(overview.textContent).not.toMatch(/原本|ハッシュ|再圧縮|画像参照/u);
    const imageSection = screen.getByRole('region', { name: '画像の差分' });
    expect(imageSection.querySelectorAll('.jc-notice')).toHaveLength(1);
    expect(document.body.textContent).not.toContain('画像ファイル内容：');
  });
});

describe('job list count matches the timeline in demo mode', () => {
  beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); });
  it('shows the fictional total on the card instead of 応募未取得', () => {
    const { container } = render(<JobCopyScreen />);
    const first = container.querySelector('.jc-job');
    expect(first?.textContent).toContain(`応募${String(demo('demo-job-001').overallApplications?.total ?? -1)}件（架空）`);
    expect(first?.textContent).not.toContain('応募未取得');
  });
});
