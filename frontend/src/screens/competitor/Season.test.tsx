// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { GoogleSection } from '../../generated/GoogleSection';
import type { GoogleSeason } from '../../generated/GoogleSeason';
import { seasonLayout } from './charts/seasonLayout';
import { GoogleTab } from './tabs/GoogleTab';

const months = Array.from({ length: 12 }, (_, i) => {
  const mean = i === 2 ? 130 : i === 7 ? 70 : 100;
  return { month: i + 1, mean, min: mean, max: mean };
});

const season = (over: Partial<GoogleSeason> = {}): GoogleSeason => ({
  keyword: '飲食店 求人',
  scope: '全国',
  monthly_avg: 2000,
  full_years: [2022, 2023, 2024, 2025],
  months,
  reliable: true,
  headline: '例年、3月ごろに最も多く（年平均比 +30%）、8月ごろに最も少ない（年平均比 −30%）。',
  caveat: null,
  peak_month: 3,
  low_month: 8,
  next_three: [
    { month: 12, index: 100 },
    { month: 1, index: 100 },
    { month: 2, index: 100 },
  ],
  current_month: 11,
  current_index: 100,
  market: {
    months: months.map((m) => ({ month: m.month, mean: m.mean })),
    n_series: 84,
    peak_month: 3,
    low_month: 8,
    headline: '職種全体の傾向では、3月ごろに多く（年平均比 +30%）、8月ごろに少ない（年平均比 −30%）。',
  },
  indeed: {
    series: [
      {
        label: '閲覧数',
        points: [
          { month: 5, year: 2025, index: 187 },
          { month: 5, year: 2026, index: 93 },
        ],
        peak_month: null,
        shift_from_google: null,
      },
    ],
    first_month: '2025-05',
    last_month: '2026-06',
    n_months: 14,
    points_per_month: [1, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1],
    span_note: 'Indeed は 2025-05〜2026-06 の 14 か月分で、暦月ごとの点数は 1〜2 点です。',
    shift_note: 'Indeed が 24 か月分たまると、山の月と Google とのずれを表示します。',
  },
  ...over,
});

const section = (s: GoogleSeason | null): GoogleSection => ({
  status: 'ok',
  keyword: '飲食店 求人',
  region: '',
  demand: { status: 'ok', region_name: null, keywords: [] },
  suggestions: { status: 'ok', region_name: null, suggestions: [] },
  season: s,
});

afterEach(cleanup);

describe('暦月の型 (全国)', () => {
  it('言えるとき: 見出し・今月の印・Indeed の点・事実の 1 文・次の 3 か月の表', () => {
    const { container } = render(<GoogleTab data={section(season())} />);
    expect(screen.getByText('暦月の型（全国）')).toBeTruthy();
    expect(screen.getByText('例年、3月ごろに最も多く（年平均比 +30%）、8月ごろに最も少ない（年平均比 −30%）。')).toBeTruthy();
    expect(screen.getByText('Indeed は 2025-05〜2026-06 の 14 か月分で、暦月ごとの点数は 1〜2 点です。')).toBeTruthy();
    expect(screen.getByText('Indeed が 24 か月分たまると、山の月と Google とのずれを表示します。')).toBeTruthy();
    expect(screen.getByText('11月（今月）')).toBeTruthy();
    expect(container.querySelector('[data-testid="season-current"]')).not.toBeNull();
    const titles = [...container.querySelectorAll('svg title')].map((t) => t.textContent);
    expect(titles).toContain('3月　年平均比 130（最小 130〜最大 130）');
    expect(titles).toContain('5月 2025年　閲覧数 年平均比 187');
    expect(titles).toContain('5月 2026年　閲覧数 年平均比 93');
    // 市場全体は 1 行の注記だけ (主にしない)
    expect(screen.queryByText('職種全体の暦月の型')).toBeNull();
  });

  it('言えないとき: 事実だけ。「ごろに最も多く」と書かず、市場全体の型を主に出す。Indeed なしなら線も文も無い', () => {
    const { container } = render(
      <GoogleTab
        data={section(
          season({
            reliable: false,
            headline: 'この職種の検索は、年による差が季節の差より大きく、時期の型は出せません。（全国の月平均 900 回・完全な年 4 年）',
            caveat: '月平均が 1,000 回に届かず、検索数の丸めの影響が季節の差と同じ大きさになります。',
            indeed: null,
          }),
        )}
      />,
    );
    expect(screen.getByText(/時期の型は出せません。（全国の月平均 900 回・完全な年 4 年）/)).toBeTruthy();
    expect(container.textContent).not.toContain('ごろに最も多く');
    expect(container.querySelectorAll('figure').length).toBe(2);
    expect(container.textContent).not.toContain('Indeed');
    expect(screen.queryByText('11月（今月）')).toBeNull();
  });

  it('season が null ならブロックを出さない', () => {
    render(<GoogleTab data={section(null)} />);
    expect(screen.queryByText('暦月の型（全国）')).toBeNull();
  });
});

describe('seasonLayout', () => {
  it('指数の軸は 20 刻みで 100 を含み、点数ぶんの Indeed の点を持つ', () => {
    const l = seasonLayout(
      months.map((m) => ({ month: m.month, mean: m.mean, range: [m.min, m.max] as [number, number] })),
      3,
      season().indeed,
    );
    expect(l?.grid.map((g) => g.value)).toEqual([60, 80, 100, 120, 140, 160, 180, 200]);
    expect(l?.indeed).toHaveLength(2);
    expect(l?.dots).toHaveLength(12);
    expect(l?.current?.cx).toBeCloseTo(85 + (791 * 2) / 11, 5);
  });
});
