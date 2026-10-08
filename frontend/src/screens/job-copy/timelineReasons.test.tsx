// @vitest-environment happy-dom
/**
 * The 応募理由 lane, the per-period reason table, the 多い応募理由 column of the cross-job overview,
 * and the category breakdown on the 応募理由 tab, with the fictional demo reasons (demoReasons.ts):
 * 9 applications with a reason out of 28; 1 has no application date.
 */
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { CopyVersion, JobCopyRecord } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { JobTimeline } from './JobTimeline';
import { JobOverview } from './JobOverview';
import { ApplicantReasons } from './ApplicantReasons';
import { CAUSAL_PATTERN, JARGON_PATTERN } from './format';

vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); });

function demo(): JobCopyRecord {
  const job = jobs.find(item => item.id === 'demo-job-001');
  if (!job) throw new Error('Missing demo job');
  return job;
}
const cells = (table: HTMLElement) => within(table).getAllByRole('row').slice(1).map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent));
const propertyNames = /oubodouki|ouboriyuu|genshokumae|kategori|hiaringu|baitaikisai/u;

describe('応募理由 lane and the per-period table', () => {
  it('counts the reasons of each period with n, and gives shares only from n=5', () => {
    render(<JobTimeline job={demo()} marketMode="demo" />);
    const table = within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getByRole('table');
    expect(cells(table)).toEqual([
      // 初回掲載 09/01〜09/14: 給与 chosen (09-02), 家から近い (09-04), 勤務地 chosen (09-10). n=3: no shares.
      ['n=3', '選択済み2件・推定1件・分類できない0件', '1件 選択1', '2件 選択1・推定1', '0件', '0件', '0件'],
      // 09/15〜09/24: してみたい, 土日休み, 特になし, 時給 (未設定 chosen), 職種興味 chosen. n=5: shares.
      ['n=5', '選択済み1件・推定3件・分類できない1件', '1件（20%） 推定1', '0件（0%）', '2件（40%） 選択1・推定1', '0件（0%）', '1件（20%） 推定1'],
      ['n=0', '記録なし', '—', '—', '—', '—', '—'],
    ]);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect([...lane.querySelectorAll('.jt-reason')].map(block => block.textContent)).toEqual(['n=3 勤務地2件（選択1・推定1）', 'n=5 職種興味2件（選択1・推定1）']);
    expect(lane.querySelector('.jt-reason')?.getAttribute('title')).toBe('初回掲載: n=3（応募）: 給与 1件（選択1・推定0）、勤務地 2件（選択1・推定1）');
    expect(screen.getByText('応募日が分からない応募理由 1件 は段と表に入れていません')).toBeTruthy();
    const section = screen.getByRole('region', { name: '期間ごとの応募理由' });
    expect(section.textContent).not.toMatch(CAUSAL_PATTERN);
    expect(section.textContent).not.toMatch(JARGON_PATTERN);
    expect(section.textContent).not.toMatch(propertyNames);
  });

  it('says 未取得 (not 0件) when reasons were not read, and draws no reason table', () => {
    render(<JobTimeline job={{ ...demo(), applicantReasons: undefined }} marketMode="demo" />);
    expect(within(screen.getByRole('group', { name: '応募理由' })).getByText('応募理由は未取得です（0件という意味ではありません）')).toBeTruthy();
    expect(screen.queryByRole('region', { name: '期間ごとの応募理由' })).toBeNull();
  });
});

describe('多い応募理由 in the cross-job overview', () => {
  it('names the top two reasons with counts and n, and 未取得 for a job without reasons', () => {
    const other = jobs.find(item => item.id !== 'demo-job-001');
    if (!other) throw new Error('Missing second demo job');
    render(<JobOverview records={[demo(), other]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    const table = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getByRole('table');
    const header = [...table.querySelectorAll('thead th')].map(cell => cell.textContent);
    expect(header.at(-1)).toBe('多い応募理由');
    const values = within(table).getAllByRole('row').slice(1).map(row => row.querySelectorAll('td')[row.querySelectorAll('td').length - 1]?.textContent);
    expect(values).toEqual(['給与 2件（選択1・推定1）・勤務地 2件（選択1・推定1）／n=9', '未取得']);
  });

  it('marks an old stored file: texts counted as 記述, choices 未取得, and no 記録なし without texts', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const legacy = { ...reasons, selections: null, items: [], sourceCounts: Object.fromEntries(Object.entries(reasons.sourceCounts).filter(([property]) => ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'].includes(property))) };
    render(<JobOverview records={[{ ...job, applicantReasons: legacy }]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    const table = within(screen.getByRole('region', { name: '求人の横断比較の表' })).getByRole('table');
    expect(table.querySelector('td.jo-reasons')?.textContent).toBe('文の記録なし（分類の選択は未取得）');
    cleanup();
    render(<JobTimeline job={{ ...job, applicantReasons: legacy }} marketMode="demo" />);
    expect(within(screen.getByRole('group', { name: '応募理由' })).getByText('応募理由の文の記録はありません（分類の選択はこのデータでは未取得です。0件という意味ではありません）')).toBeTruthy();
  });

  it('says when the texts were cut, in the overview and under the timeline lane', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const cut = { ...reasons, truncated: true };
    render(<JobOverview records={[{ ...job, applicantReasons: cut }]} onChoose={() => undefined} now={new Date('2026-10-05T12:00:00+09:00')} />);
    expect(screen.getByRole('region', { name: '求人の横断比較の表' }).querySelector('td.jo-reasons')?.textContent).toBe('給与 2件（選択1・推定1）・勤務地 2件（選択1・推定1）／n=9（記述の一部だけで集計）');
    cleanup();
    render(<JobTimeline job={{ ...job, applicantReasons: cut }} marketMode="demo" />);
    expect(screen.getByText('記述が多く一部しか読み込んでいないため、期間ごとの応募理由の数は実際より少ないことがあります')).toBeTruthy();
  });
});

describe('category breakdown on the 応募理由 tab', () => {
  it('shows counts per source, 選択済み and 推定 apart with n, and the unclassified text collapsed', () => {
    render(<ApplicantReasons job={demo()} />);
    // The header counts fields (欄), with the filled ones apart: 28 applications × 6 fields.
    expect(screen.getByText('記録欄の数（応募28件 × 6欄 = 168欄）: 記入あり12欄 · 空欄0欄 · 記録なし156欄')).toBeTruthy();
    expect(document.body.textContent).not.toContain('記録された理由');
    const sources = within(screen.getByRole('region', { name: '記録欄ごとの件数' })).getByRole('table');
    expect(within(sources).getAllByRole('row').slice(1).map(row => [row.querySelector('th')?.textContent, ...[...row.querySelectorAll('td')].map(cell => cell.textContent)])).toEqual([
      ['応募動機（HubSpot）', '4件', '0件', '24件'],
      ['応募理由（媒体記載）', '1件', '0件', '27件'],
      ['応募理由（ヒアリング）', '2件', '0件', '26件'],
      ['今の仕事・前の仕事から転職する理由', '1件', '0件', '27件'],
      ['応募理由の分類（ヒアリング）', '3件', '0件', '25件'],
      ['応募理由の分類（媒体記載）', '1件', '0件', '27件'],
    ]);
    const summary = screen.getByRole('region', { name: '応募理由の分類' });
    expect(summary.textContent).toContain('n=9（応募9件） · 選択済み3件 · キーワードで推定5件 · 分類できない1件');
    const categories = within(screen.getByRole('region', { name: '応募理由の分類の件数' })).getByRole('table');
    expect(within(categories).getAllByRole('row').slice(1).map(row => [row.querySelector('th')?.textContent, ...[...row.querySelectorAll('td')].map(cell => cell.textContent)])).toEqual([
      ['給与', '2件', '1件', '1件', '22%'],
      ['勤務地', '2件', '1件', '1件', '22%'],
      ['職種興味', '2件', '1件', '1件', '22%'],
      ['会社規模', '1件', '0件', '1件', '11%'],
      ['その他', '1件', '0件', '1件', '11%'],
    ]);
    // The transfer reason (通勤に片道1時間) is counted apart, by keywords only.
    const transfer = within(screen.getByRole('region', { name: '転職理由の分類の件数' })).getByRole('table');
    expect(within(transfer).getAllByRole('row')[2]?.textContent).toBe('勤務地1件1件n=1のため出しません');
    expect(summary.textContent).toContain('nが5件に満たないため、割合は出さず件数だけを示します。');
    const unclassified = summary.querySelector('details.ar-unclassified');
    expect(unclassified?.hasAttribute('open')).toBe(false);
    expect(unclassified?.querySelector('summary')?.textContent).toBe('分類できなかった記録を開く（1件・社内確認用）');
    expect(unclassified?.querySelector('blockquote')?.textContent).toBe('特になし');
    const shown = summary.cloneNode(true) as HTMLElement;
    shown.querySelectorAll('blockquote').forEach(node => { node.remove(); });
    expect(shown.textContent).not.toMatch(propertyNames);
    expect(shown.textContent).not.toMatch(CAUSAL_PATTERN);
    expect(shown.textContent).not.toMatch(/\d人/u);
  });

  it('never shows an internal option value whose name could not be read, and words 未設定 carefully when texts were cut', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const key = 'f'.repeat(64);
    const unsetKey = 'e'.repeat(64);
    const changed = { ...reasons, truncated: true, optionLabels: 'unavailable' as const, selections: [...(reasons.selections ?? []),
      { applicant: key, sourceProperty: 'ouboriyuukategori_hiaringu', value: 'kyuuyo_code_x', label: null, applicationDate: '2026-09-20' },
      { applicant: unsetKey, sourceProperty: 'ouboriyuukategori_hiaringu', value: 'mise', label: '未設定', applicationDate: '2026-09-20' }] };
    render(<ApplicantReasons job={{ ...job, applicantReasons: changed }} />);
    const summary = screen.getByRole('region', { name: '応募理由の分類' });
    expect(summary.textContent).not.toContain('kyuuyo_code_x');
    expect(summary.textContent).toContain('分類が選ばれているのに分類の名前を読み取れなかった応募が 1件 あります。');
    expect(summary.textContent).toContain('分類が「未設定」で、読み込めた文もない応募 1件 は数えていません（記述が上限を超えて一部を読み込んでいないため、文が記録されている応募も含まれることがあります）。');
    expect(summary.textContent).not.toContain('「未設定」で文もない');
    // The live option list could not be read: reopening may help.
    expect(summary.textContent).toContain('時間をおいて開き直すと読み取れることがあります。');
    expect(summary.textContent).toContain('分類は選ばれていますが、分類の名前を読み取れませんでした');
  });

  it('says why a chosen category has no name: not in the option list, or a stored file without names', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const unlisted = { applicant: 'f'.repeat(64), sourceProperty: 'ouboriyuukategori_hiaringu', value: 'removed_option', label: null, applicationDate: '2026-09-20' };
    // The option list was read and does not hold the value: reopening does not help.
    render(<ApplicantReasons job={{ ...job, applicantReasons: { ...reasons, optionLabels: 'read', selections: [...(reasons.selections ?? []), unlisted] } }} />);
    let summary = screen.getByRole('region', { name: '応募理由の分類' });
    expect(summary.textContent).toContain('選ばれた分類が今のHubSpotの選択肢の一覧にない応募が 1件 あります（選択肢が消されたか、名前が変わった可能性があります）。');
    expect(summary.textContent).toContain('選ばれた分類が今の選択肢の一覧にありません（選択済みには数えていません）。');
    expect(summary.textContent).not.toContain('時間をおいて開き直す');
    expect(summary.textContent).not.toContain('removed_option');
    cleanup();
    // A stored file written without the names (or before this was recorded).
    for (const optionLabels of ['not_stored', null] as const) {
      render(<ApplicantReasons job={{ ...job, applicantReasons: { ...reasons, optionLabels, selections: [...(reasons.selections ?? []), unlisted] } }} />);
      summary = screen.getByRole('region', { name: '応募理由の分類' });
      expect(summary.textContent).toContain('保存された取得データに分類の名前が入っていないため、開き直しても変わりません。');
      expect(summary.textContent).not.toContain('時間をおいて開き直す');
      cleanup();
    }
  });

  it('shows 未取得 for the sources a stored file did not hold, never 0件', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const legacy = { ...reasons, selections: null, items: reasons.items.filter(item => item.sourceProperty !== 'genshokumaeshokukaranotenshokuriyuu').map(item => ({ ...item, applicant: null })),
      sourceCounts: Object.fromEntries(Object.entries(reasons.sourceCounts).filter(([property]) => ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'].includes(property))) };
    render(<ApplicantReasons job={{ ...job, applicantReasons: legacy }} />);
    const sources = within(screen.getByRole('region', { name: '記録欄ごとの件数' })).getByRole('table');
    const rows = within(sources).getAllByRole('row').slice(1).map(row => row.textContent);
    expect(rows.slice(3)).toEqual([
      '今の仕事・前の仕事から転職する理由未取得（0件という意味ではありません）',
      '応募理由の分類（ヒアリング）未取得（0件という意味ではありません）',
      '応募理由の分類（媒体記載）未取得（0件という意味ではありません）',
    ]);
    const summary = screen.getByRole('region', { name: '応募理由の分類' });
    expect(summary.textContent).toContain('選択済みの件数は0件ではなく不明です');
    expect(summary.textContent).toContain('記述ごとに数えています');
    expect(summary.textContent).toContain('今の仕事・前の仕事から転職する理由の分類応募理由とは別に');
    expect(summary.textContent).toContain('未取得です（0件という意味ではありません）。');
  });
});

describe('reasons in 取得日の間 rows and on the as-of day', () => {
  const version = (id: string, observedAt: string, body: string): CopyVersion => ({
    id, label: id, observedAt, certainty: 'unknown', kind: 'published', source: '合成', body, applications: null, note: '', images: [],
  });
  const reasonsOn = (dates: string[]): ApplicantReasonCollection => ({
    available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-09-02T00:00:00Z', totalApplicants: dates.length, totalSourceValues: dates.length * 6,
    sourceCounts: {}, missing: 0, blank: 0, truncated: false, optionLabels: 'read', selections: [],
    items: dates.map((date, index) => ({ id: String(index).padEnd(64, '0'), applicant: String(index + 1).repeat(64), text: '家から近いため', sourceProperty: 'oubodouki', applicationDate: date, collectedAt: null, versionId: null })),
  });
  // A on 08-01 and 08-12 (same), B on 08-20 (salary changed) and 08-31 (same): the change fell
  // between the acquisitions of 08-12 and 08-20 (the same record as timelineRound7.test.tsx).
  const record = (dates: string[]): JobCopyRecord => ({
    id: 'synthetic', title: '合成配送ドライバー', company: '合成取引先', media: 'HRハッカー', mediaJobId: '12345678', location: '大分県大分市', dataSource: 'hubspot',
    versions: [
      version('a1', '2026-08-01T00:00:00Z', '給与：月給230,000円'),
      version('a2', '2026-08-12T00:00:00Z', '給与：月給230,000円'),
      version('b1', '2026-08-20T00:00:00Z', '給与：月給250,000円'),
      version('b2', '2026-08-31T00:00:00Z', '給与：月給250,000円'),
    ],
    overallApplications: { total: dates.length, missingDate: 0, fetchedAt: '2026-09-02T00:00:00Z', distributions: {}, byDate: Object.fromEntries(dates.map(date => [date, 1])) },
    applicantReasons: reasonsOn(dates),
  });
  const reasonRows = () => within(within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getByRole('table')).getAllByRole('row').slice(1)
    .map(row => [row.querySelector('th')?.textContent ?? '', row.querySelectorAll('td')[0]?.textContent ?? ''] as const);

  it('puts the reasons of the days between two acquisitions into the 取得日の間 row', () => {
    render(<JobTimeline job={record(['2026-08-15', '2026-08-18'])} marketMode="demo" showDummyBilling={false} />);
    const rows = reasonRows();
    const between = rows.filter(([label]) => label.includes('の間に変化'));
    expect(between).toHaveLength(1);
    expect(between[0]?.[1]).toBe('n=2');
    // No other row holds them.
    expect(rows.filter(([label]) => !label.includes('の間に変化')).map(([, n]) => n).every(n => n === 'n=0' || n === '')).toBe(true);
    expect(screen.queryByText(/期間の外/u)).toBeNull();
  });

  it('counts a reason dated on the as-of day in the last period, not outside', () => {
    const job = demo();
    const reasons = job.applicantReasons;
    if (!reasons) throw new Error('Missing demo reasons');
    const asOfDay = { ...reasons, items: [...reasons.items, { id: 'f'.repeat(64), applicant: 'f'.repeat(64), text: '時給が高い', sourceProperty: 'oubodouki', applicationDate: '2026-10-05', collectedAt: null, versionId: null }] };
    render(<JobTimeline job={{ ...job, applicantReasons: asOfDay }} marketMode="demo" />);
    const table = within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getByRole('table');
    expect(cells(table).at(-1)?.slice(0, 3)).toEqual(['n=1', '選択済み0件・推定1件・分類できない0件', '1件 推定1']);
  });
});
