// @vitest-environment happy-dom
/**
 * Contract conformance (2026-10-08): the screen against the media / application acquisition
 * contracts.
 * 1. A version is dated by the day it was acquired. A change is reported as 「文面を確認した日A〜文面を確認した日Bの間に
 *    変化」; the days in between are not given to either version, and after the last acquisition
 *    nothing is known (未取得).
 * 2. Images: reference changes (差し替え / 並び順) and same-reference file changes (中身) are told
 *    apart; 同じ only when both comparisons agree; a version without image data is 未取得.
 * 3. An application linked to more than one job is never put into a version's period, and totals
 *    across jobs say they can count one application more than once.
 * 6. The dummy billing can be turned off on screen and is never in a total or a sort.
 * 7. The screen says it shows a selected subset of jobs.
 * 8. The before/after sort has a visible note that the order is not a cause.
 */
import { beforeEach as pinSidebarBeforeEach, afterEach as clearSidebarAfterEach } from 'vitest';
pinSidebarBeforeEach(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
clearSidebarAfterEach(() => { localStorage.removeItem('hrhr-job-copy-sidebar-pinned'); });
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import { openOtherAnalysis } from './openOtherAnalysis.test-helper';
import type { CopyVersion, JobCopyRecord } from './data';
import type { CopyImage } from './images';
import { IMAGE_CHANGE_MARK, imageChangeKind } from './images';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { JobCopyScreen, DUMMY_BILLING_STORAGE_KEY } from './JobCopyScreen';
import { applicationsOutsideTimeline } from './applicationCountsModel';
import { overallFromLiveSummary } from './liveApplications';
import { overviewRow } from './overviewModel';
import { boundaryStatus, buildPeriods, changeKinds, periodRows, uncertainSpans, versionChanges } from './timelineModel';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

const hash = (character: string) => character.repeat(64);
const image = (reference: string, file: string, slot: number): CopyImage => ({ id: `${reference}-${String(slot)}`, url: `/api/job-copy/snapshot-image?listing_id=30&version=0&slot=${String(slot)}&image_hash=${hash(file)}`, caption: '', contentHash: hash(file), sourceReferenceHash: hash(reference), sourceSlot: slot });
const version = (id: string, observedAt: string, body: string, images?: CopyImage[], extra: Partial<CopyVersion> = {}): CopyVersion => ({
  id, label: `${observedAt.slice(0, 10).replaceAll('-', '/')}時点の求人内容`, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '', ...(images ? { images } : {}), ...extra,
});
const job = (versions: CopyVersion[], byDate: Record<string, number>, fetchedAt: string, extra: Partial<JobCopyRecord> = {}): JobCopyRecord => ({
  id: 'synthetic', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県大分市', dataSource: 'hubspot', versions,
  overallApplications: { total: Object.values(byDate).reduce((sum, count) => sum + count, 0), missingDate: 0, fetchedAt, distributions: {}, byDate }, ...extra,
});
const sameImages = [image('a', '1', 1), image('b', '2', 2)];
// Acquired 07-01, 07-15 (same content), 08-20 (salary changed); counts taken 08-25.
const captured = job([
  version('v1', '2026-07-01T00:00:00Z', '給与：月給230,000円\n仕事内容：配送', sameImages),
  version('v2', '2026-07-15T00:00:00Z', '給与：月給230,000円\n仕事内容：配送', sameImages),
  version('v3', '2026-08-20T00:00:00Z', '給与：月給250,000円\n仕事内容：配送', sameImages),
], { '2026-06-20': 1, '2026-07-05': 2, '2026-07-20': 3, '2026-08-01': 4, '2026-08-20': 5, '2026-08-23': 6 }, '2026-08-25T00:00:00Z');

describe('1. versions dated by acquisition day', () => {
  it('runs each recorded body until the next record and keeps the last one current', () => {
    const periods = buildPeriods(captured, '2026-08-25');
    expect(periods.map(period => [period.versionId, period.start, period.end, period.days, period.basis])).toEqual([
      ['v1', '2026-07-01', '2026-07-15', 14, 'captured'],
      ['v2', '2026-07-15', '2026-08-20', 36, 'captured'],
      ['v3', '2026-08-20', null, 6, 'captured'],
    ]);
    expect(uncertainSpans(captured, '2026-08-25', periods)).toEqual([]);
  });

  it('counts all dated applications after the first record in exactly one body', () => {
    const rows = periodRows(captured, { asOf: '2026-08-25', dummyBilling: false });
    expect(rows.map(row => [row.kind, row.start, row.days, row.applications])).toEqual([
      ['period', '2026-07-01', 14, 2], ['period', '2026-07-15', 36, 7], ['period', '2026-08-20', 6, 11],
    ]);
    expect(rows.reduce((sum, row) => sum + (row.applications ?? 0), 0)).toBe(20);
    expect(applicationsOutsideTimeline(captured, new Date('2026-08-25T03:00:00Z'))).toBe(1);
  });

  it('shows the changed body from its record day, retaining the earlier unknown count', () => {
    render(<JobTimeline job={captured} marketMode="demo" now={new Date('2026-08-25T03:00:00Z')} showDummyBilling={false} />);
    const lane = screen.getByRole('group', { name: '掲載期間' });
    expect(lane.querySelectorAll('.jt-zone')).toHaveLength(0);
    expect(lane.querySelector('.jt-unacquired')).toBeNull();
    expect(screen.getByRole('region', { name: 'タイムライン' }).textContent).toContain('最初の本文の記録日より前の応募 1件');
    const third = within(screen.getByRole('group', { name: '本文' })).getAllByRole('button')[2];
    if (!third) throw new Error('missing third body');
    fireEvent.click(third);
    expect(screen.getByRole('region', { name: '選んだ本文' }).textContent).toContain('2026/08/20 に確認');
    const counts = within(screen.getByRole('region', { name: '期間比較表の数値' })).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[1]?.textContent);
    expect(counts).toEqual(['2件', '7件', '11件']);
  });

  it('anchors the overview before/after windows at 文面を確認した日A and 文面を確認した日B and has no change when nothing changed', () => {
    const row = overviewRow(captured, { now: new Date('2026-08-25T03:00:00Z'), dummyBilling: false });
    expect(row.lastChange).toEqual({ from: '2026-07-15', to: '2026-08-20', exact: false });
    // Before: 07-02 to 07-14 (v1 and v2 had the same content; 07-01 is the first acquisition day and
    // 07-15 is 文面を確認した日A, so neither counts): 07-05 2 = 2件 / 13日.
    expect(row.before).toEqual({ days: 14, applications: 0, perDay: 0 });
    // After: the later content is known on no whole day (08-20 is 文面を確認した日B and the last acquisition),
    // so the 5 applications on 08-20 are not given to it.
    expect(row.after).toEqual({ days: 6, applications: 11, perDay: 11 / 6 });
    const unchanged = job([version('u1', '2026-07-01T00:00:00Z', '給与：月給230,000円', sameImages), version('u2', '2026-07-20T00:00:00Z', '給与：月給230,000円', sameImages)], { '2026-07-05': 1 }, '2026-07-25T00:00:00Z');
    const none = overviewRow(unchanged, { dummyBilling: false });
    expect([none.lastChange, none.kinds, none.before, none.after]).toEqual([null, [], null, null]);
    render(<JobOverview records={[unchanged]} onChoose={() => undefined} showDummyBilling={false} />);
    const table = screen.getByRole('region', { name: '求人の横断比較の表' });
    expect(table.textContent).toContain('変化は見つかっていません');
    expect(table.textContent).not.toMatch(/変更日|判定できない変更/u);
  });
});

describe('2. images: references and file contents', () => {
  const base = { id: 'p', images: sameImages };
  it('tells 差し替え, 並び順, 中身, 同じ, 未取得 and 不明 apart, judging by the CSV image URLs', () => {
    expect(imageChangeKind(base, { id: 'q', images: [image('a', '1', 1), image('c', '3', 2)] })).toBe('replaced');
    expect(imageChangeKind(base, { id: 'q', images: [image('b', '2', 1), image('a', '1', 2)] })).toBe('reordered');
    // Same references in the same places, but the file behind reference b changed.
    expect(imageChangeKind(base, { id: 'q', images: [image('a', '1', 1), image('b', '9', 2)] })).toBe('content');
    expect(imageChangeKind(base, { id: 'q', images: [image('a', '1', 1), image('b', '2', 2)] })).toBe('same');
    // Past files not saved: the same URLs in the same places are 同じ (the URLs decide).
    expect(imageChangeKind({ ...base, historicalImageBytesAvailable: false }, { id: 'q', images: sameImages })).toBe('same');
    // No images is a known state: 画像なし → 画像あり is a change, なし → なし is the same.
    expect(imageChangeKind({ id: 'p', imageReferences: [], historicalImageBytesAvailable: false }, { id: 'q', imageReferences: [{ referenceHash: 'a'.repeat(64), slot: 1 }] })).toBe('replaced');
    expect(imageChangeKind({ id: 'p', imageReferences: [{ referenceHash: 'a'.repeat(64), slot: 1 }] }, { id: 'q', imageReferences: [] })).toBe('replaced');
    expect(imageChangeKind({ id: 'p', imageReferences: [], historicalImageBytesAvailable: false }, { id: 'q', imageReferences: [] })).toBe('same');
    expect(imageChangeKind(base, { id: 'q' })).toBe('missing');
    expect(imageChangeKind(undefined, { id: 'q' })).toBe('missing');
    expect(imageChangeKind({ id: 'p' }, { id: 'q', images: sameImages })).toBe('unknown');
    expect(Object.values(IMAGE_CHANGE_MARK).map(mark => mark.text)).toEqual(['最初', '未取得', '差し替え', '並び順', '中身', '同じ', '不明']);
  });

  it('counts a same-URL file change as an image change in the lane and the change kinds, and never says 変更はありません then', () => {
    const record = job([version('i1', '2026-07-01T00:00:00Z', '本文', sameImages), version('i2', '2026-07-10T00:00:00Z', '本文', [image('a', '1', 1), image('b', '9', 2)]), version('i3', '2026-07-20T00:00:00Z', '本文')], {}, '2026-07-20T00:00:00Z');
    const changes = versionChanges(record);
    expect(changes.map(change => change.imageChange)).toEqual(['initial', 'content', 'missing']);
    expect(changes[1] && changeKinds(changes[1])).toEqual(['画像']);
    render(<JobTimeline job={record} marketMode="demo" showDummyBilling={false} />);
    const marks = within(screen.getByRole('group', { name: '画像' })).getAllByRole('button');
    expect(marks.map(mark => [mark.textContent, mark.getAttribute('aria-label')])).toEqual([
      ['最初', `${changes[0]?.label ?? ''}の画像：最初の本文`],
      ['中身', `${changes[1]?.label ?? ''}の画像：同じ場所の画像の中身が変わりました`],
      ['未取得', `${changes[2]?.label ?? ''}の画像：この本文に対応する画像は未取得です`],
    ]);
    const second = marks[1];
    if (!second) throw new Error('missing the second 画像 mark');
    fireEvent.click(second);
    const panel = screen.getByRole('region', { name: '選んだ本文' });
    expect(panel.textContent).not.toContain('前回から本文・画像の変更はありません');
    expect(panel.textContent).toContain('本文は前回と同じです。画像：同じ場所の画像の中身が変わりました');
    // The days between i1 and i2 belong to neither version (the images changed in between).
    expect(periodRows(record, { asOf: '2026-07-20', dummyBilling: false }).map(row => row.kind)).toEqual(['period', 'period', 'period']);
  });
});

describe('3. applications linked to more than one job', () => {
  it('reads the multi-job counts from HubSpot and leaves them out of every version row', () => {
    const overall = overallFromLiveSummary({ total: 5, missing_date: 1, by_date: { '2026-07-01': 2, '2026-07-02': 2 }, dimensions: {}, multi_listing_by_date: { '2026-07-02': 1 }, multi_listing_missing_date: 1 }, '2026-07-05T00:00:00Z');
    expect(overall?.multiListing).toEqual({ byDate: { '2026-07-02': 1 }, missingDate: 1 });
    // A count larger than the dated applications is not trusted.
    expect(overallFromLiveSummary({ total: 2, missing_date: 0, by_date: { '2026-07-01': 2 }, dimensions: {}, multi_listing_by_date: { '2026-07-01': 3 } }, '2026-07-02T00:00:00Z')).toBeNull();
    if (!overall) throw new Error('summary not read');
    const record = job([version('m1', '2026-07-01T00:00:00Z', '本文', sameImages), version('m2', '2026-07-05T00:00:00Z', '本文', sameImages)], {}, '2026-07-05T00:00:00Z', { overallApplications: overall });
    const rows = periodRows(record, { asOf: '2026-07-05', dummyBilling: false });
    // 07-02 has 2 applications; the multi-job one is left out of m1's row.
    expect(rows.map(row => [row.versionId, row.applications])).toEqual([['m1', 3], ['m2', 0]]);
    // 1 undated + 2 on the first acquisition day + 1 multi-job dated (the undated multi-job one is
    // already in the undated count) = 4.
    expect(applicationsOutsideTimeline(record, new Date('2026-07-05T03:00:00Z'))).toBe(2);
    render(<JobTimeline job={record} marketMode="demo" showDummyBilling={false} />);
    expect(screen.getByRole('region', { name: 'タイムライン' }).textContent).toContain('複数の求人に関連する応募 2件 は期間比較表に入れていません');
  });

  it('says when it cannot tell multi-job applications apart', () => {
    render(<JobTimeline job={captured} marketMode="demo" showDummyBilling={false} />);
    expect(screen.getByRole('region', { name: 'タイムライン' }).textContent).toContain('複数の求人に関連する応募を見分ける情報を取得していないため、期間比較表の件数に含まれている場合があります');
  });
});

describe('6–8. dummy billing switch, selected subset, sort note', () => {
  beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); vi.stubGlobal('fetch', vi.fn()); });
  // タイムラインは「ほかの分析」の横画面にある。開いていなければ開く。
  const billingLane = () => {
    if (!screen.queryByRole('dialog', { name: 'ほかの分析' })) openOtherAnalysis('タイムライン');
    return screen.getByRole('group', { name: '課金' });
  };
  const chooseJob = (title: string) => {
    const button = [...document.querySelectorAll<HTMLButtonElement>('.jc-job')].find(item => item.textContent.includes(title));
    if (!button) throw new Error(`missing ${title}`);
    fireEvent.click(button);
  };

  it('shows the dummy billing by default, hides it with the on-screen switch and remembers the choice', () => {
    window.localStorage.removeItem(DUMMY_BILLING_STORAGE_KEY);
    const view = render(<JobCopyScreen />);
    const toggle = screen.getByRole<HTMLInputElement>('checkbox', { name: '仮の課金データを表示' });
    expect(toggle.checked).toBe(true);
    chooseJob('倉庫内ピッキングスタッフ');
    expect(billingLane().querySelectorAll('.jt-billing-dummy').length).toBeGreaterThan(0);
    fireEvent.click(toggle);
    expect(window.localStorage.getItem(DUMMY_BILLING_STORAGE_KEY)).toBe('0');
    expect(billingLane().querySelectorAll('.jt-billing-dummy')).toHaveLength(0);
    expect(billingLane().textContent).toContain('課金データなし（0円という意味ではありません）');
    view.unmount();
    render(<JobCopyScreen />);
    expect(screen.getByRole<HTMLInputElement>('checkbox', { name: '仮の課金データを表示' }).checked).toBe(false);
    window.localStorage.removeItem(DUMMY_BILLING_STORAGE_KEY);
  });

  it('still renders and switches when the browser storage throws', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('blocked'); });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('blocked'); });
    render(<JobCopyScreen />);
    const toggle = screen.getByRole<HTMLInputElement>('checkbox', { name: '仮の課金データを表示' });
    expect(toggle.checked).toBe(true);
    fireEvent.click(toggle);
    expect(toggle.checked).toBe(false);
  });

  it('says the screen shows a selected subset of jobs, and keeps the sort note visible', () => {
    render(<JobCopyScreen />);
    expect(screen.getByText('この画面は、選んで取り込んだ一部の求人（8件）だけを表示しています。管理しているすべての求人ではありません。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '横断比較' }));
    expect(screen.getByText('並び順は数の大小で並べただけです。応募が増えた・減った理由を示すものではありません。')).toBeTruthy();
    expect(screen.getByRole('combobox', { name: '並び替え' }).textContent).toContain('変化後（後の文面を確認した日の翌日から）の1日あたり応募が多い順');
  });
});

describe('5. an HRハッカー row and a billing CSV row for the same days', () => {
  it('shows a conflict instead of choosing one of them', () => {
    const demo = jobs.find(item => item.id === 'demo-job-001');
    if (!demo) throw new Error('missing demo-job-001');
    render(<JobTimeline job={demo} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} showDummyBilling={false} billing={[{ source: 'csv', start: '2026-09-01', end: '2026-09-14', amountYen: 33000, media: 'HRハッカー', sourceRow: 2 }]} />);
    expect([...screen.getByRole('group', { name: '課金' }).querySelectorAll('.jt-billing')].map(bar => bar.textContent)).toEqual(['3万3,000円', '架空 3万円', '架空 4万5,000円', '架空 1万2,000円']);
    expect(screen.getByText(/HRハッカーの期間別実績と読み込んだ課金CSVに、同じ日を含む課金があります/u)).toBeTruthy();
    const first = within(screen.getByRole('region', { name: '期間比較表の数値' })).getAllByRole('row')[1];
    expect(first?.querySelectorAll('td')[3]?.textContent).toBe('HRハッカーの実績と課金CSVが重なっています（どちらも合計していません）');
  });
});

describe('real snapshot shape: past files not saved, same CSV text and image URLs', () => {
  const ref = (slot: number) => ({ referenceHash: String(slot).repeat(64), slot });
  const version = (id: string, observedAt: string, references: { referenceHash: string; slot: number }[], past: boolean): CopyVersion => ({
    id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: 'HRハッカーCSV', body: '仕事内容\n配送\n給与：月給250,000円', applications: null, note: '',
    imageReferences: references, images: [], ...(past ? { historicalImageBytesAvailable: false } : {}),
  });
  const job = (references: { referenceHash: string; slot: number }[], nowReferences = references): JobCopyRecord => ({
    id: 'real-shape', title: 't', company: 'c', media: 'HRハッカー', mediaJobId: '1', location: '',
    versions: [version('v1', '2026-10-03T01:00:00+09:00', references, true), version('v2', '2026-10-04T01:00:00+09:00', nowReferences, false)],
  });
  it('is 変化なし (no span between the two acquisitions), with or without images', () => {
    for (const references of [[ref(1), ref(2)], []]) {
      const record = job(references);
      const change = versionChanges(record)[1];
      expect(change && boundaryStatus(change)).toBe('same');
      expect(uncertainSpans(record, '2026-10-05').filter(span => span.kind === 'between')).toHaveLength(0);
    }
  });
  it('画像なし → 画像あり is a change', () => {
    const record = job([], [ref(1)]);
    const change = versionChanges(record)[1];
    expect(change?.imageChange).toBe('replaced');
    expect(change && boundaryStatus(change)).toBe('changed');
  });
});
