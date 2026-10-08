// @vitest-environment happy-dom
/**
 * The 応募理由 lane, the per-period reason table, the 多い応募理由 column of the cross-job overview,
 * and the category breakdown on the 応募理由 tab, with the fictional demo reasons (demoReasons.ts):
 * 9 applications with a reason out of 28; 1 has no application date.
 */
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
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
      ['n=3', '選択済み2件・推定1件・分類できない0件', '1件', '2件', '0件', '0件', '0件'],
      // 09/15〜09/24: してみたい, 土日休み, 特になし, 時給 (未設定 chosen), 職種興味 chosen. n=5: shares.
      ['n=5', '選択済み1件・推定3件・分類できない1件', '1件（20%）', '0件（0%）', '2件（40%）', '0件（0%）', '1件（20%）'],
      ['n=0', '記録なし', '—', '—', '—', '—', '—'],
    ]);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect([...lane.querySelectorAll('.jt-reason')].map(block => block.textContent)).toEqual(['n=3 勤務地2件', 'n=5 職種興味2件']);
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
    expect(values).toEqual(['給与 2件・勤務地 2件（n=9）', '未取得']);
  });
});

describe('category breakdown on the 応募理由 tab', () => {
  it('shows counts per source, 選択済み and 推定 apart with n, and the unclassified text collapsed', () => {
    render(<ApplicantReasons job={demo()} />);
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
