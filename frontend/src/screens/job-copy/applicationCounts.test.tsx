// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';
import { jobApplicationTotal, linkedApplicationCount, noLinkedApplicationsMessage, unmatchedApplicationCount } from './applicationCountsModel';
import { jobs } from './data';
import type { JobCopyRecord } from './data';

const response = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } });
const capturedAt = '2026-10-06T00:00:00.000Z';
const versionId = `capture-synthetic-counts-${capturedAt}`;
const empty = { denominator: 0, categories: [] };
const snapshot = () => ({ schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-counts', hubspotListingId: '30', title: '合成の件数確認', company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '東京都', body: '合成の本文', images: [] }] },
  results: [{ listing_id: '30', summary: { total: 34, missing_date: 0, by_date: { '2026-09-10': 34 }, dimensions: { gender: { 男性: 34 }, prefecture: { 東京都: 32, 大分県: 2 }, municipality: { '東京都 / 新宿区西新宿2-8-1 ○○ビル301': 31, '東京都 / 港区芝公園4-2-8': 1, '大分県 / 大分市府内町1-1': 2 } },
    joint_demographics: { total: 34, cells: [{ gender: '男性', age: '30代', prefecture: '東京都', municipality: '東京都 / 新宿区西新宿2-8-1 ○○ビル301', count: 34 }] } },
  dated_comparison: { total: 34, unknown: 34, basis: '合成の日付対応', by_version: { [versionId]: { count: 0, dimensions: { gender: empty, age: empty, prefecture: empty, municipality: empty } } }, daily_representatives: {} } }] });

beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); });
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('one application denominator across panels', () => {
  it('shows the same 34 unmatched applications on 本文・画像 and 応募者構成, with the plain no-linked sentence', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(response(snapshot()))));
    const { container } = render(<JobCopyScreen />);
    const summary = await screen.findByRole('region', { name: '版別の応募状況' });
    expect(within(summary).getByRole('status').textContent).toBe('この版に結びつく応募はまだありません（求人全体では34件）');
    expect(summary.querySelector('.jc-counts')?.textContent).toContain('どの版への応募か不明（求人全体）34件');
    fireEvent.click(screen.getByRole('tab', { name: '応募分析' }));
    fireEvent.click(screen.getByRole('tab', { name: '応募者構成' }));
    const overall = await screen.findByRole('region', { name: '求人全体の実応募者構成' });
    expect(overall.textContent).toContain('応募34件 · 応募日不明0件 · どの版への応募か不明34件');
    expect(screen.getByText(/どの版への応募か不明: 34件/u)).toBeDefined();
    expect(screen.getAllByText('この版に結びつく応募はまだありません（求人全体では34件）。割合は算出できません。')).toHaveLength(4);
    const municipality = within(overall).getByRole('region', { name: '求人全体の市区町村' });
    expect([...municipality.querySelectorAll('.ac-chart-row')].map(row => row.textContent)).toEqual(['東京都新宿区31件 (91.2%)', 'その他3件 (8.8%)']);
    expect(screen.getByRole('region', { name: '実データの取得範囲' }).textContent).toContain('34');
    // 元の住所は画面のどこにも出ない（閉じた逆検索の選択肢も含む）
    expect(container.innerHTML).not.toMatch(/西新宿|○○ビル|芝公園|府内町/u);
  });
});

describe('application count model', () => {
  const demo = jobs.find(job => job.id === 'demo-job-001');
  it('derives the demo job totals from the version counts (12+0+0, 9+0+2, 4+0+1, 0)', () => {
    if (!demo) throw new Error('demo-job-001 missing');
    expect(unmatchedApplicationCount(demo)).toBe(3);
    expect(jobApplicationTotal(demo)).toBe(28);
    expect(demo.versions.map(linkedApplicationCount)).toEqual([12, 9, 4, 0]);
  });
  it('prefers the HubSpot job-level values and treats an unattributed HubSpot job as fully unmatched', () => {
    const live = { id: 'x', versions: [], overallApplications: { total: 34, missingDate: 2, fetchedAt: capturedAt, distributions: {} } } as unknown as JobCopyRecord;
    expect(unmatchedApplicationCount(live)).toBe(34);
    expect(unmatchedApplicationCount({ ...live, attributionUnknown: 31 })).toBe(31);
    expect(jobApplicationTotal(live)).toBe(34);
    expect(unmatchedApplicationCount({ ...live, overallApplications: undefined } as unknown as JobCopyRecord)).toBeNull();
    expect(noLinkedApplicationsMessage(34)).toBe('この版に結びつく応募はまだありません（求人全体では34件）');
    expect(noLinkedApplicationsMessage(null)).toBe('この版に結びつく応募はまだありません（求人全体の件数は未取得）');
    expect(noLinkedApplicationsMessage(34)).not.toMatch(/効果|確実に|必ず|100%/u);
  });
});
