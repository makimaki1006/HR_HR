// @vitest-environment happy-dom
/**
 * Review round 5 (2026-10-08): the 本文 mark for a version whose body could not be read, a spoken
 * label that matches the visible mark, plain names for version kinds and application counts, and
 * explanations that can be read without hovering.
 */
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { JobCopyScreen } from './JobCopyScreen';
import { JARGON_PATTERN } from './format';
import { bodyMark, versionChanges } from './timelineModel';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', () => ({ apiGet: api }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const driver = (): JobCopyRecord => {
  const job = jobs.find(item => item.id === 'demo-job-001');
  if (!job) throw new Error('Missing demo-job-001');
  return job;
};
const version = (id: string, observedAt: string, body: string): JobCopyRecord['versions'][number] => ({ id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '' });

describe('本文 lane mark', () => {
  // v0 → the same text with CRLF line ends → an empty body (could not be read).
  const body = '仕事内容：配送\n給与：月給25万円\n休日：土日';
  const job: JobCopyRecord = { ...driver(), versions: [
    version('初回', '2026-09-01T00:00:00Z', body),
    version('改行違い', '2026-09-10T00:00:00Z', body.replace(/\n/g, '\r\n')),
    version('空本文', '2026-09-20T00:00:00Z', ''),
    version('同じ本文', '2026-09-25T00:00:00Z', ''),
  ] };

  it('shows 比べられない (not 追加0・削除0) for an empty body and speaks what is shown', () => {
    expect(versionChanges(job).map(change => change.bodyStatus)).toEqual(['initial', 'format_only', 'unavailable', 'unavailable']);
    render(<JobTimeline job={job} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} />);
    const marks = [...screen.getByRole('group', { name: '本文' }).querySelectorAll('button')];
    expect(marks.map(mark => mark.textContent)).toEqual(['最初', '改行のみ', '比べられない', '比べられない']);
    expect(marks.map(mark => mark.getAttribute('aria-label'))).toEqual([
      '初回の本文：最初の版',
      '改行違いの本文：改行だけが違います',
      '空本文の本文：本文が無いため、前の版と比べられません',
      '同じ本文の本文：本文が無いため、前の版と比べられません',
    ]);
    expect(marks.map(mark => mark.textContent).join('')).not.toContain('追加0');
    expect(marks.some(mark => (mark.getAttribute('aria-label') ?? '').includes('行追加'))).toBe(false);
  });

  it('gives the same counts in the text and the spoken label for a changed body', () => {
    expect(bodyMark({ index: 1, bodyStatus: 'changed', bodyAdded: 2, bodyRemoved: 1 })).toEqual({ text: '追加2・削除1', spoken: '2行追加・1行削除' });
    expect(bodyMark({ index: 1, bodyStatus: 'unchanged', bodyAdded: 0, bodyRemoved: 0 })).toEqual({ text: '同じ', spoken: '前の版と同じ' });
    expect(bodyMark({ index: 0, bodyStatus: 'unavailable', bodyAdded: 0, bodyRemoved: 0 })).toEqual({ text: '最初', spoken: '最初の版' });
  });
});

describe('explanations that can be read without hovering', () => {
  it('explains 約 and 未取得 in the period table with visible text, not title attributes', () => {
    // demo-job-001: 09-01〜09-14, 09-15〜09-24, 09-25〜. One billing row of 45,000円 for 09-15〜09-30
    // covers two version periods, so both get a prorated (約) share.
    // HRハッカー実績は外す（同じ日に両方あると、どちらも合計しない「重なり」になるため）。
    render(<JobTimeline job={{ ...driver(), hrhPerformance: undefined }} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} billing={[{ source: 'csv', start: '2026-09-15', end: '2026-09-30', amountYen: 45000 }]} />);
    const table = screen.getByRole('region', { name: '期間比較表の数値' });
    expect(table.querySelectorAll('[title]')).toHaveLength(0);
    expect(table.textContent).toContain('約2万8,125円');
    expect(screen.getByText('「約」の付いた課金額は、課金の期間と版の期間がずれているため、日数で割って配分した金額です。')).toBeTruthy();
  });

  it('shows the 未取得 note when application dates were never fetched', () => {
    const job: JobCopyRecord = { ...driver(), versions: driver().versions.map(item => ({ ...item, applications: null })) };
    delete job.overallApplications;
    render(<JobTimeline job={job} marketMode="demo" now={new Date('2026-10-05T03:00:00Z')} />);
    const table = screen.getByRole('region', { name: '期間比較表の数値' });
    expect(table.textContent).toContain('未取得');
    expect(screen.getByText('「未取得」は応募日ごとの件数を取得していないという意味です。0件という意味ではありません。')).toBeTruthy();
  });

  it('explains short windows on the cross-job overview below the table', () => {
    const job: JobCopyRecord = { id: 'synthetic', title: '合成ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: 'S-1', location: '大分県大分市',
      versions: [version('a', '2026-08-01T00:00:00Z', '給与：月給230,000円'), version('b', '2026-08-20T00:00:00Z', '給与：月給250,000円')],
      dataSource: 'hubspot', overallApplications: { total: 4, missingDate: 0, fetchedAt: '2026-08-20T00:00:00Z', distributions: {}, byDate: { '2026-08-10': 2, '2026-08-20': 2 } } };
    render(<JobOverview records={[job]} onChoose={() => undefined} />);
    const table = screen.getByRole('region', { name: '求人の横断比較の表' });
    expect(within(table).getAllByRole('row')[1]?.textContent).toContain('期間が短いため比べません2件 / 1日');
    expect(table.querySelectorAll('td[title]')).toHaveLength(0);
    expect(document.querySelector('.jo-short-note')?.textContent).toContain('ある日数分だけで1日あたりを数えています');
  });

  it('opens the screen explanations by click (試作版, 取得日時, どの版への応募か不明)', () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    render(<JobCopyScreen />);
    const summaries = [...document.querySelectorAll('.jc-infotip > summary')].map(node => node.textContent);
    expect(summaries).toContain('試作版 ⓘ');
    expect(summaries).toContain('取得日時: 2026/09/25 12:30 JST ⓘ');
    expect(document.querySelector('.jc-mode')?.getAttribute('title')).toBeNull();
    expect(document.querySelector('.jc-record-meta [title]')).toBeNull();
  });
});

describe('plain names for version kinds and counts', () => {
  it('treats the old internal names as developer terms', () => {
    for (const word of ['媒体取得版', '媒体CSV取得版', '受信版', '確定対応', '推定対応', '選択版の確定＋推定対応']) expect(word).toMatch(JARGON_PATTERN);
    for (const word of ['媒体から取り込んだ求人', '確認待ちの文面', '応募日で結びついた応募', '気づいた日の版で数えた応募', '選んだ版に結びついた応募']) expect(word).not.toMatch(JARGON_PATTERN);
  });

  it('names a received text 確認待ちの文面 and shows no developer terms after adding it', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    api.mockResolvedValue({ ok: true, data: { titles: [], prefectures: [] } });
    render(<JobCopyScreen />);
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'データ取込' })); await Promise.resolve(); });
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '外部文面を照合する' })); await Promise.resolve(); });
    fireEvent.change(screen.getByLabelText('受け取った文面'), { target: { value: '仕事内容：配送\n給与：月給30万円' } });
    fireEvent.click(screen.getByRole('button', { name: '現在の本文と比較' }));
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '確認待ちの文面としてデモ履歴に追加' })); await Promise.resolve(); });
    expect(screen.getByText('確認待ちの文面として画面内のデモ履歴に追加しました。媒体での掲載確認・HubSpot保存は行っていません。')).toBeTruthy();
    expect(screen.getByRole('heading', { level: 2, name: '確認待ちの文面 1の文面' })).toBeTruthy();
    const counts = document.querySelector('.jc-counts');
    expect(counts).toBeNull();
    expect(document.body.textContent).not.toMatch(JARGON_PATTERN);
  });

  it('labels the per-version counts in plain words', () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    render(<JobCopyScreen />);
    fireEvent.click(screen.getByRole('tab', { name: '求人内容' }));
    const counts = document.querySelector('.jc-counts')?.textContent ?? '';
    expect(counts).toMatch(/^応募日で結びついた応募\d+件気づいた日の版で数えた応募\d+件どの版への応募か不明（求人全体）\d+件どんな応募か ⓘ/);
  });
});
