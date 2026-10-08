// @vitest-environment happy-dom
/**
 * Review round 7 (2026-10-08): acquisition days next to a change are not given to either version,
 * no 「1日あたり」 for periods shorter than 7 days (the same rule as the cross-job overview), the
 * dummy billing notes say where the amounts are shown, the real-data banner says the amounts are
 * made up, the printable report reads the applied billing CSV, area totals are counted from the
 * protected cells, and more address forms are masked in the reason texts.
 */
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CopyVersion, JobCopyRecord } from './data';
import type { CopyImage } from './images';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { JobCopyScreen, DUMMY_BILLING_STORAGE_KEY } from './JobCopyScreen';
import { ConsultantReview } from './ConsultantReview';
import { reportBillingText } from './consultantReviewModel';
import { roundApplicantAreasInRecord } from './applicantArea';
import { maskPersonalDetails } from './personalText';
import { applicationsOutsidePeriods, countRanges, MIN_RATE_DAYS, periodRows } from './timelineModel';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const hash = (character: string) => character.repeat(64);
const sameImages: CopyImage[] = [{ id: 'a-1', url: `/api/job-copy/snapshot-image?listing_id=30&version=0&slot=1&image_hash=${hash('1')}`, caption: '', contentHash: hash('1'), sourceReferenceHash: hash('a'), sourceSlot: 1 }];
const version = (id: string, observedAt: string, body: string): CopyVersion => ({
  id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '', images: sameImages,
});
const job = (versions: CopyVersion[], byDate: Record<string, number>, fetchedAt: string): JobCopyRecord => ({
  id: 'synthetic', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県大分市', dataSource: 'hubspot', versions,
  overallApplications: { total: Object.values(byDate).reduce((sum, count) => sum + count, 0), missingDate: 0, fetchedAt, distributions: {}, byDate },
});

describe('acquisition days next to a change', () => {
  // A on 08-01 and again on 08-12 (same), B on 08-20 (salary changed) and again on 08-31 (same).
  const record = job([
    version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円'),
    version('a2', '2026-08-12T00:00:00Z', '給与：月給230,000円'),
    version('b1', '2026-08-20T00:00:00Z', '給与：月給250,000円'),
    version('b2', '2026-08-31T00:00:00Z', '給与：月給250,000円'),
  ], { '2026-08-01': 1, '2026-08-05': 2, '2026-08-12': 3, '2026-08-15': 4, '2026-08-20': 5, '2026-08-25': 6, '2026-08-31': 7 }, '2026-09-02T00:00:00Z');

  it('counts an acquisition day for a version only when the acquisitions on both sides found the same content', () => {
    expect(countRanges(record, '2026-09-02')).toEqual([
      { start: '2026-08-02', end: '2026-08-12' },
      { start: '2026-08-12', end: '2026-08-12' },
      { start: '2026-08-21', end: '2026-08-31' },
      { start: '2026-08-31', end: '2026-08-31' },
    ]);
    const rows = periodRows(record, { asOf: '2026-09-02', dummyBilling: false });
    expect(rows.map(row => [row.kind, row.start, row.lastDay, row.days, row.applications, row.perDay])).toEqual([
      ['period', '2026-08-02', '2026-08-11', 10, 2, 0.2],
      ['period', '2026-08-12', '2026-08-12', 0, 0, null],
      // 取得日A (08-12) and 取得日B (08-20) are in the 取得日の間 row: the change may fall on either day.
      ['between', '2026-08-12', '2026-08-20', 9, 12, null],
      ['period', '2026-08-21', '2026-08-30', 10, 6, 0.6],
      ['period', '2026-08-31', '2026-08-31', 0, 0, null],
      ['unacquired', '2026-08-31', '2026-09-02', 3, 7, null],
    ]);
    // No day is in two rows; the only day outside is the first acquisition day (08-01).
    for (const [index, row] of rows.entries()) {
      const next = rows[index + 1];
      if (next) expect(row.end).toBe(next.start);
    }
    expect(applicationsOutsidePeriods(record, rows)).toBe(1);
  });

  it('starts the overview after-window the day after 取得日B and ends the before-window the day before 取得日A', () => {
    render(<JobOverview records={[record]} onChoose={() => undefined} showDummyBilling={false} now={new Date('2026-09-02T03:00:00Z')} />);
    const row = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getAllByRole('row')[1];
    // before: 08-02〜08-11 (2件 / 10日), after: 08-21〜08-30 (6件 / 10日).
    expect(row?.textContent).toContain('0.20件/日2件 / 10日');
    expect(row?.textContent).toContain('0.60件/日6件 / 10日');
    expect(screen.getByRole('columnheader', { name: /前の取得日の前日まで/u })).toBeTruthy();
  });
});

describe('no 1日あたり for short periods', () => {
  it('says 期間が短いため比べません for a version row shorter than 7 days, as the overview does', () => {
    const record = job([
      version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円'),
      version('a2', '2026-08-05T00:00:00Z', '給与：月給230,000円'),
      version('b1', '2026-08-10T00:00:00Z', '給与：月給250,000円'),
    ], { '2026-08-03': 2 }, '2026-08-12T00:00:00Z');
    const rows = periodRows(record, { asOf: '2026-08-12', dummyBilling: false });
    expect([rows[0]?.days, rows[0]?.applications, rows[0]?.perDay, rows[0]?.shortPeriod]).toEqual([3, 2, null, true]);
    expect(MIN_RATE_DAYS).toBe(7);
    render(<JobTimeline job={record} marketMode="demo" now={new Date('2026-08-12T03:00:00Z')} showDummyBilling={false} />);
    const table = screen.getByRole('region', { name: '期間比較表の数値' });
    const first = within(table).getAllByRole('row')[1];
    expect(first?.textContent).toContain('2件期間が短いため比べません');
    expect(first?.textContent).not.toContain('件/日');
    expect(screen.getByRole('region', { name: 'タイムライン' }).textContent).toContain('7日に満たない期間は1日あたりを出さず、比べません');
  });
});

describe('dummy billing notes', () => {
  it('does not use the word レーン and says where the made-up amounts are shown', () => {
    const record = job([version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円')], { '2026-08-03': 1 }, '2026-08-12T00:00:00Z');
    render(<><JobTimeline job={record} marketMode="demo" now={new Date('2026-08-12T03:00:00Z')} showDummyBilling /><JobOverview records={[record]} onChoose={() => undefined} showDummyBilling /></>);
    const notes = [...document.querySelectorAll('.jt-dummy-billing')].map(node => node.textContent);
    expect(notes).toHaveLength(2);
    for (const note of notes) {
      expect(note).not.toContain('レーン');
      expect(note).toContain('「課金」の段');
    }
    expect(document.body.textContent).not.toContain('レーン');
  });
});

describe('real-data banner', () => {
  const capturedAt = '2026-10-06T00:00:00.000Z';
  const snapshot = { schemaVersion: 1, capturedAt, capture_bundle: { schemaVersion: 1, capturedAt, jobs: [{ id: 'synthetic-banner', hubspotListingId: '30', title: '合成の求人', company: '合成会社', media: 'HRハッカー', mediaJobId: '12345678', location: '東京都', body: '合成の本文', images: [] }] },
    results: [{ listing_id: '30', summary: { total: 1, missing_date: 0, by_date: { '2026-09-10': 1 }, dimensions: { gender: { 男性: 1 } } }, dated_comparison: null }] };
  beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy'); try { window.localStorage.removeItem(DUMMY_BILLING_STORAGE_KEY); } catch { /* storage may be blocked */ } });

  it('says the billing amounts are made up while 仮の課金データを表示 is on, and not after it is turned off', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(JSON.stringify(snapshot), { status: 200, headers: { 'content-type': 'application/json' } }))));
    const { container } = render(<JobCopyScreen />);
    await screen.findByText('実データ（取得済み）');
    const banner = () => container.querySelector('.jc-demo')?.textContent ?? '';
    const toggle = screen.getByRole<HTMLInputElement>('checkbox', { name: '仮の課金データを表示' });
    expect(toggle.checked).toBe(true);
    expect(banner()).toContain('仮の金額（ダミー）');
    expect(banner()).toContain('実際の請求額ではありません');
    fireEvent.click(toggle);
    expect(banner()).not.toContain('ダミー');
  });
});

describe('printable report billing', () => {
  const record = job([version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円')], { '2026-08-03': 1 }, '2026-08-12T00:00:00Z');
  const draft = { stage: 'plan', target: 'body', fields: {}, selection: ['a1', 'a1'] as [string, string] };

  it('reads the billing CSV rows the user applied, the same rows the timeline shows', () => {
    const csv = [{ source: 'csv' as const, start: '2026-08-01', end: '2026-08-15', amountYen: 31000 }, { source: 'csv' as const, start: '2026-08-16', end: '2026-08-31', amountYen: null }];
    expect(reportBillingText(record, undefined).heading).toBe('課金情報は未取得');
    const text = reportBillingText(record, csv);
    expect(text.heading).toBe('読み込んだ課金CSVあり');
    expect(text.detail).toContain('2行・2026/08/01〜2026/08/31・金額の分かる行の合計 3万1,000円（金額の記載がない行あり）');
    render(<ConsultantReview job={record} draft={draft} onDraft={() => undefined} billing={csv} />);
    const report = screen.getByRole('region', { name: '顧客報告と検証記録' });
    expect(report.textContent).not.toContain('課金情報は未取得');
    expect(report.textContent).toContain('3万1,000円');
    // Overlapping rows are not added up.
    expect(reportBillingText(record, [csv[0], { source: 'csv', start: '2026-08-10', end: '2026-08-20', amountYen: 5000 }].filter(Boolean) as typeof csv).detail).toContain('期間が重なる行があるため合計していません');
  });
});

describe('area totals from the protected cells', () => {
  it('does not leave a remainder that names the hidden cells\' area', () => {
    // 由布市 3 = 女性・60代 1 + 男性・30代 2; 大分市 3 = 男性・20代 3.
    const record = roundApplicantAreasInRecord({ ...job([], {}, '2026-08-12T00:00:00Z'),
      overallApplications: { total: 6, missingDate: 0, fetchedAt: '2026-08-12T00:00:00Z', byDate: {}, distributions: {
        municipality: { total: 6, categories: [{ category: '大分県由布市', count: 3, percentage: 50 }, { category: '大分県大分市', count: 3, percentage: 50 }] },
      } },
      jointDemographics: { total: 6, cells: [
        { gender: '女性', age: '60代', prefecture: '大分県', municipality: '大分県由布市', count: 1 },
        { gender: '男性', age: '30代', prefecture: '大分県', municipality: '大分県由布市', count: 2 },
        { gender: '男性', age: '20代', prefecture: '大分県', municipality: '大分県大分市', count: 3 },
      ] } });
    expect(record.overallApplications?.distributions.municipality?.categories.map(row => [row.category, row.count])).toEqual([['大分県大分市', 3], ['その他', 3]]);
    expect(JSON.stringify(record)).not.toContain('由布市');
  });
});

describe('reason text masking (same rule as the server)', () => {
  it('masks kanji house numbers, a building with a room number and a town after the city', () => {
    expect(maskPersonalDetails('府内町三丁目十番一号です')).toBe('＊＊です');
    expect(maskPersonalDetails('3丁目一緒に働ける人がいるため')).toBe('＊＊一緒に働ける人がいるため');
    expect(maskPersonalDetails('府内ビル201に住んでいます')).toBe('＊＊に住んでいます');
    expect(maskPersonalDetails('コーポ北浜102から通います')).toBe('＊＊から通います');
    expect(maskPersonalDetails('大分市府内町に住んでいます')).toBe('大分市＊＊に住んでいます');
    expect(maskPersonalDetails('大分県別府市北浜町の近く')).toBe('大分県別府市＊＊の近く');
    for (const text of ['大分市内に住んでいます', '大分市在住で、市区町村の補助を使いたい', '別府市役所の近く', 'ビルの清掃を3年していました', '週3日から働けるため']) {
      expect(maskPersonalDetails(text)).toBe(text);
    }
  });
});
