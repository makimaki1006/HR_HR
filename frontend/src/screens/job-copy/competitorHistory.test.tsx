// @vitest-environment happy-dom
// 競合の推移: 入口の URL・月ごとの表・条件が違う月の印・記録なし・出来事・報告の1枚。
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CompetitorMonth } from '../../generated/CompetitorMonth';
import { CompetitorHistory, CompetitorMonthDetails } from './CompetitorHistory';
import { competitorEntryUrl, competitorRows } from './competitorHistoryModel';
import type { JobCopyRecord } from './data';
import { buildListingEvents, eventChecks } from './listingEventsModel';
import { buildReport } from './reportSlidesModel';
import type { ReportInput, ReportOptions } from './reportSlidesModel';

afterEach(cleanup);

// 架空の記録。9月と10月は同じ条件、11月は上位の件数と地域が違う。
const month = (m: string, day: string, total: number, median: number | null, own: number | null, patch: Partial<CompetitorMonth['conditions']> = {}): CompetitorMonth => ({
  month: m, run_at: `${day}T16:20:00+09:00`, by: 'writer@example.test',
  conditions: { title: 'フォークリフト 旭川', employment_type: '正社員', prefecture: '北海道', municipality: '旭川市', top_n: 30, source: 'Indeed SP の一覧CSV', ...patch },
  count: { total, salary_parsed: total - 6, popular: 12, super_popular: 3 },
  salary: { unit: '万円', all: { min_mean: 22.8, min_median: median, max_mean: 29.1, max_median: 28 }, popular: { min_mean: 24.1, min_median: 24, max_mean: 30.5, max_median: 30 } },
  own_salary: own === null ? null : { min: own, max: 23 },
  keywords_top5: [['未経験', 41], ['賞与', 33], ['車通勤', 29], ['資格取得', 18], ['夜勤', 12]],
  csv_url: null,
});
const latest = month('2026-11', '2026-11-15', 86, 23.5, 22, { top_n: 45, municipality: '札幌市' });
const records = [latest, month('2026-09', '2026-09-08', 78, 22.5, 20.1), month('2026-10', '2026-10-06', 80, null, null)];
const job = (patch: Partial<JobCopyRecord> = {}): JobCopyRecord => ({
  id: 'hubspot-history-30', hubspotId: '30', dataSource: 'hubspot', title: '顧客A フォークリフト', company: '顧客A', media: 'HRハッカー', mediaJobId: 'HR-30', location: '北海道旭川市',
  listingPrefecture: '北海道', listingMunicipality: '旭川市',
  listingSummary: { occupation: 'フォークリフト', location: '北海道旭川市', salary_kind: '月給', salary_min: '220000', salary_max: '230000', hours: null, holidays: null, body_changed_at: null },
  overallApplications: { total: 5, missingDate: 0, fetchedAt: '2026-11-16T09:00:00+09:00', distributions: {}, byDate: { '2026-09-03': 1, '2026-09-30': 2, '2026-11-01': 2, '2026-11-20': 1, '2026-12-01': 4 } },
  competitorMonthly: records, versions: [], ...patch,
});

describe('競合の推移のデータ', () => {
  it('古い月から並べ、その月の応募を数え、前の月と違う条件を挙げる', () => {
    expect(competitorRows(job()).map(row => [row.month, row.total, row.median, row.own, row.applications, row.changed])).toEqual([
      ['2026-09', 78, 22.5, 20.1, 3, []],
      ['2026-10', 80, null, null, 0, []],
      ['2026-11', 86, 23.5, 22, 3, ['地域', '上位の件数']],
    ]);
  });
  it('応募の日別データが無いときは、応募を 0 件にしない', () => {
    expect(competitorRows({ competitorMonthly: records }).map(row => row.applications)).toEqual([null, null, null]);
  });
  it('入口は、求人の番号と条件を付けた競合調査の URL。HubSpot の求人でなければ出さない', () => {
    expect(competitorEntryUrl(job())).toBe(`/app/competitor?${new URLSearchParams({ listing: '30', listing_name: '顧客A フォークリフト', job_name: 'フォークリフト', prefecture: '北海道', municipality: '旭川市' }).toString()}`);
    expect(competitorEntryUrl({ ...job(), hubspotId: '' })).toBeNull();
    expect(competitorEntryUrl(job({ hubspotId: 'demo-1' }))).toBeNull();
  });
});

describe('競合の推移の表', () => {
  it('月ごとに件数・給与の中央値・自社の給与・応募を並べ、条件が違う月に印を付ける', () => {
    render(<CompetitorHistory job={job()} />);
    const cells = (m: string) => [...screen.getByTestId(`competitor-row-${m}`).querySelectorAll('th,td')].map(cell => cell.textContent);
    expect(cells('2026-09')).toEqual(['2026年9月', '78件', '22.5万円', '20.1万円', '3件', '最初の記録']);
    expect(cells('2026-10')).toEqual(['2026年10月', '80件', '—', '—', '0件', '前の月と同じ']);
    expect(cells('2026-11')).toEqual(['2026年11月', '86件', '23.5万円', '22万円', '3件', '前の月と違う：地域・上位の件数']);
    const link = screen.getByRole('link', { name: /この求人で競合調査をする/ });
    expect(link.getAttribute('href')).toBe(competitorEntryUrl(job()));
    expect(link.getAttribute('target')).toBe('_blank');
    // 判定の言葉は出さない。
    expect(screen.getByTestId('competitor-history').textContent).not.toMatch(/高い|低い|良い|悪い|上回|下回/);
  });
  it('記録が無ければ 1 行だけ出す（入口は残す）', () => {
    render(<CompetitorHistory job={job({ competitorMonthly: [] })} />);
    const card = screen.getByTestId('competitor-history');
    expect(within(card).getByRole('status').textContent).toBe('まだ競合調査の記録はありません');
    expect(card.querySelector('table')).toBeNull();
    expect(within(card).getByRole('link', { name: /この求人で競合調査をする/ })).toBeTruthy();
  });
  it('その月の記録は、件数・給与・よく出る言葉 5 つ・条件を出す', () => {
    render(<CompetitorMonthDetails entry={latest} />);
    const text = screen.getByTestId('competitor-month').textContent;
    expect(text).toContain('86件（給与が読めた求人 80件・人気の表示 12件・とても人気の表示 3件）');
    expect(text).toContain('中央値 23.5万円 ／ 平均 22.8万円');
    expect(text).toContain('22万円 〜 23万円');
    expect(text).toContain('未経験（41件）、賞与（33件）、車通勤（29件）、資格取得（18件）、夜勤（12件）');
    expect(text).toContain('検索の言葉「フォークリフト 旭川」 ／ 地域：北海道札幌市 ／ 上位 45件 ／ Indeed SP の一覧CSV');
  });
});

describe('求人の出来事', () => {
  const withVersion = job({ versions: [{ id: 'v1', label: 'v1', observedAt: '2026-08-01T09:00:00+09:00', publishedFrom: '2026-08-01T09:00:00+09:00', certainty: 'confirmed', kind: 'published', source: 'test', body: '仕事内容\n給与：月給 22万円', applications: null, note: '' }] });
  const events = buildListingEvents(withVersion, []);
  it('記録した日に「競合調査」を置く', () => {
    expect(events.filter(event => event.kind === 'competitor').map(event => [event.date, event.title, event.summary, event.competitorMonth])).toEqual([
      ['2026-09-08', '競合調査', '競合 78件・給与の中央値（下限） 22.5万円', '2026-09'],
      ['2026-10-06', '競合調査', '競合 80件', '2026-10'],
      ['2026-11-15', '競合調査', '競合 86件・給与の中央値（下限） 23.5万円', '2026-11'],
    ]);
  });
  it('競合調査は求人の変更ではないので、ほかの出来事の「ほかの変更」に数えない', () => {
    const start = events.find(event => event.kind === 'start');
    if (!start) throw new Error('掲載開始の出来事がありません');
    expect(start.date).toBe('2026-08-01');
    const near = buildListingEvents(job({ ...withVersion, competitorMonthly: [month('2026-08', '2026-08-10', 70, 22, 20)] }), []);
    const check = eventChecks(near.find(event => event.kind === 'start') ?? start, near, [], { state: 'unset' }, '2026-11-16').find(item => item.key === 'other');
    expect(check?.text).toBe('前後4週にほかの変更はありません。');
  });
});

describe('報告のスライド「周りの求人の様子」', () => {
  const input = (patch: Partial<ReportInput> = {}): ReportInput => ({
    scope: 'listing', id: '30', name: '顧客A フォークリフト', company: '顧客A', overlap: {}, asOf: '2026-11-16', outcomes: null,
    listings: [{ id: '30', title: '顧客A フォークリフト', media: 'hrh', ended: false, endedOn: null, byDate: { '2026-11-01': 2 }, billing: [], billingUnavailable: false, events: { listingId: '30', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-08-01', statusEvents: [] } }],
    applicants: { rows: null, reasons: null, missingReason: 'テスト' }, competitor: records, ...patch,
  });
  const options: ReportOptions = { from: '2026-10-19', to: '2026-11-15', addressee: '', subject: '', owner: '', sender: '', createdOn: '2026-11-16', nextActions: [], nextHeadline: '' };
  it('期間の終わりまでのいちばん新しい記録を 1 枚に写す', () => {
    const slide = buildReport(input(), options).slides.find(item => item.id === 'around');
    expect(slide).toMatchObject({
      name: '周りの求人の様子',
      headline: '11月15日に調べた周りの求人は 86件、給与の下限の中央値は 23.5万円でした',
      kpis: [
        { label: '周りの求人', value: '86件', sub: null },
        { label: '周りの求人の給与（下限の中央値）', value: '23.5万円', sub: null },
        { label: '貴社の求人の給与（下限）', value: '22万円', sub: '11/15 時点' },
      ],
      bars: [{ label: '未経験', count: 41 }, { label: '賞与', count: 33 }, { label: '車通勤', count: 29 }, { label: '資格取得', count: 18 }, { label: '夜勤', count: 12 }],
    });
    // 期間が 11/10 までなら、10月の記録（給与は読めていない）を使う。
    const earlier = buildReport(input(), { ...options, to: '2026-11-10' }).slides.find(item => item.id === 'around');
    expect(earlier).toMatchObject({ headline: '10月6日に調べた周りの求人は 80件でした', kpis: [{ label: '周りの求人', value: '80件', sub: null }] });
  });
  it('記録が無ければスライドを出さず、社内向けに理由を出す。案件の報告にも入れない', () => {
    const none = buildReport(input({ competitor: [] }), options);
    expect(none.slides.some(item => item.id === 'around')).toBe(false);
    expect(none.excluded).toContainEqual({ name: '周りの求人の様子', reason: '競合調査の記録がありません' });
    const deal = buildReport(input({ scope: 'deal' }), options);
    expect(deal.slides.some(item => item.id === 'around')).toBe(false);
    expect(deal.excluded).toContainEqual({ name: '周りの求人の様子', reason: '競合調査は求人ごとの記録のため、案件の報告には入れていません（求人の報告で出せます）' });
  });
});

describe('出来事の一覧で開く', () => {
  it('「競合調査」の行を開くと、その月の記録が出る（前後4週の比較は出さない）', async () => {
    const { ListingOverview } = await import('./ListingOverview');
    const one = job({ competitorMonthly: [latest], versions: [{ id: 'v1', label: 'v1', observedAt: '2026-08-01T09:00:00+09:00', publishedFrom: '2026-08-01T09:00:00+09:00', certainty: 'confirmed', kind: 'published', source: 'test', body: '仕事内容\n給与：月給 22万円', applications: null, note: '' }] });
    const events = buildListingEvents(one, []);
    const target = events.find(event => event.kind === 'competitor');
    const Harness = () => <ListingOverview job={one} events={events} billing={[]} market={{ state: 'unset' }} selectedId={target?.id ?? null} onSelect={() => undefined} onCompare={() => undefined} onOpenDrawer={() => undefined} />;
    render(<Harness />);
    const row = screen.getByRole('button', { name: /競合調査/ });
    expect(row.textContent).toContain('競合 86件・給与の中央値（下限） 23.5万円');
    expect(screen.getByTestId('competitor-month').textContent).toContain('未経験（41件）');
    expect(screen.queryByTestId('event-costs')).toBeNull();
    fireEvent.click(row);
  });
});
