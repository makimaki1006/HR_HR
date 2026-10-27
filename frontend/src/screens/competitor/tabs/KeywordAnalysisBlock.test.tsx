// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { KeywordAnalysis } from '../../../generated/KeywordAnalysis';
import { multiLineLayout } from '../charts/multiLine';
import { KeywordAnalysisBlock } from './KeywordAnalysisBlock';

// 架空のデータ (実在の職種・企業は含めない)。
function make(): KeywordAnalysis {
  return {
    search_trend: {
      status: 'ok',
      scope: '全国の検索数（都道府県別ではありません）',
      months: ['2026-06', '2026-07'],
      series: [
        { label: 'テスト職 求人', variant: 'job', values: [2000, 3000], latest: 3000, latest_month: '2026-07', yoy_pct: -14.4, peak_month: 1, competition: 'HIGH' },
        { label: 'テスト職', variant: 'name', values: [9000, null], latest: 12100, latest_month: '2026-07', yoy_pct: 2.04, peak_month: 5, competition: 'LOW' },
      ],
    },
    term_shifts: {
      status: 'ok',
      scope: '全国・この職種の検索語に占める割合',
      before_period: '2026-01〜2026-03',
      after_period: '2026-06〜2026-08',
      rising: [{ term: '一般語', before_pct: 5.3, after_pct: 19.9, diff_pt: 14.6 }],
      falling: [],
      months: ['2026-07', '2026-08'],
      lines: [{ term: '主語', values: [40, 30.5] }],
      thin_note: 'この職種は最新月の検索語が12語と少なく、割合が大きく振れることがあります。',
    },
    pref_terms: {
      status: 'ok',
      prefecture: '大阪府',
      month: '2026-08',
      compare_month: '2025-08',
      compare_kind: '12か月前',
      listed_clicks: 100,
      rows: [
        { rank: 1, term: '主語', clicks: 34802, share_pct: 79.36, compare_share_pct: 60, change_pt: 19.36, out_of_range: false },
        { rank: 2, term: '新語', clicks: 1000, share_pct: 2.28, compare_share_pct: null, change_pt: null, out_of_range: true },
      ],
    },
    overlay: {
      status: 'ok',
      base_month: '2026-06',
      months: ['2026-06', '2026-07'],
      indeed_label: 'Indeedの求人を見た人数（大阪府）',
      google_label: 'Googleの検索数 「テスト職 求人」（全国）',
      indeed_index: [100, 150],
      google_index: [100, 150],
      indeed_raw: [500, 750],
      google_raw: [2000, 3000],
      caption: 'Indeedは選んだ都道府県、Googleは全国。閲覧数は応募数ではありません。',
    },
    caveats: ['注意A', '注意B'],
  };
}

afterEach(cleanup);

const rowText = (cell: HTMLElement): string[] =>
  Array.from(cell.closest('tr')?.querySelectorAll('td') ?? []).map((c) => c.textContent);

const first = (els: HTMLElement[]): HTMLElement => {
  const el = els[0];
  if (!el) throw new Error('要素がありません');
  return el;
};

describe('KeywordAnalysisBlock', () => {
  it('4つの部分を正確な数値で表示する', () => {
    render(<KeywordAnalysisBlock data={make()} />);
    expect(screen.getByText('全国の検索数（都道府県別ではありません）')).toBeTruthy();
    expect(rowText(first(screen.getAllByRole('cell', { name: 'テスト職 求人' })))).toEqual(['テスト職 求人', '2026-07', '3,000', '-14.4%', '1月', '強い']);
    expect(rowText(first(screen.getAllByRole('cell', { name: 'テスト職' })))).toEqual(['テスト職', '2026-07', '12,100', '+2.0%', '5月', '弱い']);
    expect(screen.getByText('2026-01〜2026-03と2026-06〜2026-08の割合を比べています。')).toBeTruthy();
    expect(rowText(screen.getByRole('cell', { name: '一般語' }))).toEqual(['一般語', '5.3%', '19.9%', '+14.6']);
    expect(screen.getByText('この職種は最新月の検索語が12語と少なく、割合が大きく振れることがあります。')).toBeTruthy();
    expect(rowText(first(screen.getAllByRole('cell', { name: '主語' })))).toEqual(['1', '主語', '34,802', '79.4%', '60.0%', '+19.4']);
    expect(rowText(screen.getByRole('cell', { name: '新語' }))).toEqual(['2', '新語', '1,000', '2.3%', '圏外', '圏外']);
    expect(screen.getByText('比べる月：2025-08（12か月前）')).toBeTruthy();
    expect(screen.getByText('Indeedは選んだ都道府県、Googleは全国。閲覧数は応募数ではありません。')).toBeTruthy();
    expect(screen.getByText('2026-06を100とした指数です。')).toBeTruthy();
    expect(screen.getByText('Indeedの求人を見た人数（大阪府）')).toBeTruthy();
    expect(screen.getByText('注意B')).toBeTruthy();
  });

  it('見出しに職種名を出さない', () => {
    render(<KeywordAnalysisBlock data={make()} />);
    const headings = screen.getAllByRole('heading').map((h) => h.textContent);
    expect(headings).toEqual([
      'キーワードの推移・分析',
      'Google検索数の推移',
      'Indeedの検索語の伸びと落ち',
      '選んだ都道府県の検索語',
      'Indeed閲覧数とGoogle検索数の重ね表示',
    ]);
  });

  it('欠けた部分は 1 文だけ出し、ほかは残す', () => {
    const d = make();
    d.search_trend = { status: 'unavailable', message: 'この職種のGoogle検索数のデータはありません。' };
    d.term_shifts = { status: 'unavailable', message: 'この職種のIndeedの検索語の推移データはありません。' };
    d.pref_terms = { status: 'unavailable', message: '都道府県を選ぶと、その都道府県の検索語を表示します（全国では表示しません）。' };
    d.overlay = { status: 'unavailable', message: 'Google検索数のデータがないため、重ねて表示できません。' };
    render(<KeywordAnalysisBlock data={d} />);
    for (const m of [
      'この職種のGoogle検索数のデータはありません。',
      'この職種のIndeedの検索語の推移データはありません。',
      '都道府県を選ぶと、その都道府県の検索語を表示します（全国では表示しません）。',
      'Google検索数のデータがないため、重ねて表示できません。',
    ]) {
      expect(screen.getByText(m)).toBeTruthy();
    }
    expect(screen.queryAllByRole('table')).toHaveLength(0);
    expect(screen.getByText('注意A')).toBeTruthy();
  });
});

describe('multiLineLayout', () => {
  it('共通の縦軸で欠測は線を切り、観測値が無ければ null', () => {
    const l = multiLineLayout(['a', 'b', 'c'], [
      { label: 'x', values: [100, null, 50] },
      { label: 'y', values: [10, 20, 30] },
    ]);
    expect(l?.step).toBe(25);
    expect(l?.ymax).toBe(100);
    expect(l?.lines[0]?.segments).toHaveLength(0);
    expect(l?.lines[0]?.dots).toHaveLength(2);
    expect(l?.lines[1]?.segments).toHaveLength(1);
    expect(l?.lines[1]?.dots[0]?.y).toBeCloseTo(24 + 212 * 0.9, 6);
    // 月ラベルは最後の月を必ず含み、そこから等間隔 (14 か月 → 2 か月おきで 2026-08 を含む)。
    const m14 = Array.from({ length: 14 }, (_, i) => `m${String(i)}`);
    const l14 = multiLineLayout(m14, [{ label: 'x', values: m14.map(() => 1) }]);
    expect(l14?.monthLabels.map((m) => m.text)).toEqual(['m1', 'm3', 'm5', 'm7', 'm9', 'm11', 'm13']);
    expect(l14?.monthLabels.at(-1)?.end).toBe(true);
    expect(multiLineLayout(['a'], [{ label: 'x', values: [null] }])).toBeNull();
  });
});

describe('MultiLineChart の防御とホバー表示', () => {
  it('名前の空の系列・点が 1 つも無い系列は描かず、凡例にも出さない。点は「月　系列: 値」', () => {
    const a = make();
    if (a.search_trend.status !== 'ok') throw new Error('fixture');
    a.search_trend.series = [
      { label: '', variant: 'x', values: [10, 20], latest: 20, latest_month: '2026-07', yoy_pct: null, peak_month: null, competition: 'LOW' },
      { label: '点なし', variant: 'y', values: [null, null], latest: null, latest_month: '', yoy_pct: null, peak_month: null, competition: 'LOW' },
      { label: 'テスト職 求人', variant: 'job', values: [2000, 3000], latest: 3000, latest_month: '2026-07', yoy_pct: null, peak_month: null, competition: 'HIGH' },
    ];
    render(<KeywordAnalysisBlock data={a} />);
    const svg = screen.getByRole('img', { name: 'Google検索数の推移' });
    expect([...svg.querySelectorAll('g[data-series]')].map((g) => g.getAttribute('data-series'))).toEqual(['テスト職 求人']);
    expect(svg.textContent).not.toContain('点なし');
    const titles = [...svg.querySelectorAll('circle > title')].map((t) => t.textContent);
    expect(titles).toEqual(['2026-06　テスト職 求人: 2,000 回/月', '2026-07　テスト職 求人: 3,000 回/月']);
  });
});
