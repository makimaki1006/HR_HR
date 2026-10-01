// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RdCompetitorsResponse } from '../../generated/RdCompetitorsResponse';
import type { RdTalentPoolExpansionResponse } from '../../generated/RdTalentPoolExpansionResponse';

const charts = vi.hoisted((): { byId: Record<string, unknown> } => ({ byId: {} }));

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string; option: unknown }) => {
    charts.byId[props.testId] = props.option;
    return <div data-testid={props.testId} />;
  },
}));

import * as fx from './fixtures';
import {
  rankColor,
  renderCompetitors,
  renderConditionGap,
  renderDifficulty,
  renderInflowPlaceholder,
  renderInsights,
  renderMarketTrend,
  renderOpportunityMap,
  renderTalentPool,
  renderTalentPoolExpansion,
  severityClass,
  type PanelView,
} from './panels';

afterEach(() => {
  cleanup();
  charts.byId = {};
});

const mount = (view: PanelView): void => {
  render(<div>{view.body}</div>);
};
const text = (id: string): string | null => screen.getByTestId(id).textContent;
const cls = (id: string): string => screen.getByTestId(id).className;
const texts = (id: string): (string | null)[] => screen.getAllByTestId(id).map((n) => n.textContent);

describe('Panel 1 difficulty', () => {
  it('formats like the old page: score 1 digit, share x100 with 2 digits and %, counts with units', () => {
    const view = renderDifficulty(fx.difficulty);
    mount(view);
    expect(view.statusText).toBe('完了');
    expect(text('rd-difficulty-metrics-score_per_10k')).toBe('1.7');
    expect(text('rd-difficulty-metrics-hw_count')).toBe('12,345 件');
    expect(text('rd-difficulty-metrics-population')).toBe('人口 987,654');
    expect(text('rd-difficulty-metrics-area_share_of_national')).toBe('1.23%');
    expect(text('rd-difficulty-metrics-national_hw_count')).toBe('全国 1,004,000 件');
    expect(text('rd-difficulty-rank_label')).toBe('激戦');
    expect(text('rd-difficulty-so_what')).toBe('📝 採用競合が多い傾向があります。');
  });

  it('colors the rank label per label', () => {
    expect(rankColor('非常に激戦')).toBe('text-red-400');
    expect(rankColor('激戦')).toBe('text-orange-400');
    expect(rankColor('平均的')).toBe('text-yellow-300');
    expect(rankColor('穏やか')).toBe('text-green-400');
    expect(rankColor('穴場（低競争）')).toBe('text-blue-400');
    expect(rankColor('データ不足')).toBe('text-slate-300');
    mount(renderDifficulty(fx.difficulty));
    expect(cls('rd-difficulty-rank_label')).toContain('text-orange-400');
  });

  it('shows a dash for an empty rank label and omits the so_what box when empty', () => {
    mount(renderDifficulty({ ...fx.difficulty, rank_label: '', so_what: '' }));
    expect(text('rd-difficulty-rank_label')).toBe('—');
    expect(screen.queryByTestId('rd-difficulty-so_what')).toBeNull();
  });
});

describe('Panel 2 talent pool', () => {
  it('shows a negative difference in orange without a plus sign', () => {
    mount(renderTalentPool(fx.talentPool));
    expect(text('rd-talent_pool-metrics-day_population')).toBe('1,234,567');
    expect(text('rd-talent_pool-metrics-night_population')).toBe('1,300,000');
    expect(text('rd-talent_pool-metrics-commuter_inflow')).toBe('-65,433');
    expect(cls('rd-talent_pool-metrics-commuter_inflow')).toContain('text-orange-400');
    expect(text('rd-talent_pool-metrics-day_night_ratio')).toBe('0.95');
  });

  it('shows a zero difference in blue with a plus sign, and a dash for ratio 0', () => {
    mount(
      renderTalentPool({
        ...fx.talentPool,
        metrics: { ...fx.talentPool.metrics, commuter_inflow: 0, day_night_ratio: 0 },
      }),
    );
    expect(text('rd-talent_pool-metrics-commuter_inflow')).toBe('+0');
    expect(cls('rd-talent_pool-metrics-commuter_inflow')).toContain('text-blue-400');
    expect(text('rd-talent_pool-metrics-day_night_ratio')).toBe('—');
  });
});

describe('Panel 3 inflow', () => {
  it('is the under-development placeholder', () => {
    const view = renderInflowPlaceholder();
    mount(view);
    expect(view.statusText).toBe('開発中');
    expect(document.body.textContent).toContain('🚧 開発中');
  });
});

describe('Panel 4 competitors', () => {
  it('renders one row per company with the old number formats and the report link', () => {
    const view = renderCompetitors(fx.competitors);
    mount(view);
    expect(view.statusText).toBe('完了（2社）');
    expect(screen.getAllByTestId('rd-competitors-row')).toHaveLength(2);
    const names = screen.getAllByTestId('rd-competitors-name');
    expect(names.map((n) => n.textContent)).toEqual(['テスト株式会社', '番号なし法人']);
    const link = names[0]?.querySelector('a');
    expect(link?.getAttribute('href')).toBe('/report/company/1234567890123');
    expect(link?.getAttribute('target')).toBe('_blank');
    expect(names[1]?.querySelector('a')).toBeNull();
    expect(texts('rd-competitors-hw_postings_count')).toEqual(['1,234', '3']);
    expect(texts('rd-competitors-employees')).toEqual(['12,345', '50']);
    expect(texts('rd-competitors-sales_amount')).toEqual(['987,654', '0']);
    expect(texts('rd-competitors-features')).toEqual(['介護 / 売上: 10億〜50億 / 信用 72.5', '']);
  });

  it('shows the 0-件 message and status when there are no companies', () => {
    const view = renderCompetitors({ ...fx.competitors, companies: [] });
    expect(view.statusText).toBe('完了（0件）');
    mount(view);
    expect(document.body.textContent).toContain('該当する競合企業がありません');
  });

  it('turns a non-array companies value into データ形式不正', () => {
    const bad = { ...fx.competitors, companies: null } as unknown as RdCompetitorsResponse;
    const view = renderCompetitors(bad);
    expect(view.status).toBe('error');
    mount(view);
    expect(document.body.textContent).toContain('❌ 取得失敗: データ形式不正');
  });
});

describe('Panel 5 condition gap', () => {
  it('shows benchmarks in man-yen / days / months and the colored differences', () => {
    mount(renderConditionGap(fx.conditionGap));
    expect(text('rd-condition_gap-industry_median-sample_size')).toBe('(n=12,345)');
    expect(text('rd-condition_gap-industry_median-annual_income')).toBe('450万');
    expect(text('rd-condition_gap-industry_median-annual_holidays')).toBe('115日');
    expect(text('rd-condition_gap-industry_median-bonus_months')).toBe('2.4ヶ月');
    // own 350万 / 125日 / 2.0ヶ月 against the industry median
    expect(text('rd-condition_gap-gap_industry-annual_income')).toBe('-100万');
    expect(cls('rd-condition_gap-gap_industry-annual_income')).toContain('text-red-400');
    expect(text('rd-condition_gap-gap_industry-annual_holidays')).toBe('+10日');
    expect(cls('rd-condition_gap-gap_industry-annual_holidays')).toContain('text-green-400');
    expect(text('rd-condition_gap-gap_industry-bonus_months')).toBe('-0.4ヶ月');
    // all-industry
    expect(text('rd-condition_gap-all_industry_median-annual_income')).toBe('400万');
    expect(text('rd-condition_gap-gap_all-annual_income')).toBe('-50万');
    expect(text('rd-condition_gap-gap_all-annual_holidays')).toBe('+5日');
    // the benchmark bonus is 0 -> no difference is computed
    expect(text('rd-condition_gap-all_industry_median-bonus_months')).toBe('0.0ヶ月');
    expect(text('rd-condition_gap-gap_all-bonus_months')).toBe('—');
    expect(text('rd-condition_gap-interpretation')).toBe('📝 年収は業界中央値を下回る傾向があります。');
  });

  it('shows dashes for every difference when no own conditions were entered', () => {
    mount(
      renderConditionGap({
        ...fx.conditionGap,
        company: { annual_income_estimated: 0, annual_holidays: 0, bonus_months: 0, salary_min: 0 },
      }),
    );
    for (const id of ['annual_income', 'annual_holidays', 'bonus_months']) {
      expect(text(`rd-condition_gap-gap_industry-${id}`)).toBe('—');
      expect(text(`rd-condition_gap-gap_all-${id}`)).toBe('—');
    }
  });
});

describe('Panel 6 market trend', () => {
  it('feeds months / counts into the chart and labels a sample series', () => {
    const view = renderMarketTrend(fx.marketTrend);
    mount(view);
    expect(view.statusText).toBe('完了');
    expect(text('rd-market_trend-growth_rate_pct')).toBe('期間中の変動: +25.0%');
    expect(screen.getByTestId('rd-market_trend-is_sample')).toBeTruthy();
    expect(text('rd-market_trend-data_source')).toBe('データ源: ts_turso_salary');
    const option = charts.byId['rd-chart-trend'] as {
      xAxis: { data: string[] };
      series: { data: number[]; itemStyle: { color: string } }[];
    };
    expect(option.xAxis.data).toEqual(['2026-04', '2026-05', '2026-06']);
    expect(option.series[0]?.data).toEqual([100, 110, 125]);
    expect(option.series[0]?.itemStyle.color).toBe('#0ea5e9');
  });

  it('uses the 6ヶ月前比 label and blue for a non-sample series; no banner', () => {
    mount(renderMarketTrend({ ...fx.marketTrend, is_sample: false, growth_rate_pct: -3.456 }));
    expect(text('rd-market_trend-growth_rate_pct')).toBe('6ヶ月前比: -3.5%');
    expect(screen.queryByTestId('rd-market_trend-is_sample')).toBeNull();
    const option = charts.byId['rd-chart-trend'] as { series: { itemStyle: { color: string } }[] };
    expect(option.series[0]?.itemStyle.color).toBe('#3b82f6');
  });

  it('shows 完了（データなし） for empty series', () => {
    const view = renderMarketTrend({ ...fx.marketTrend, months: [], counts: [] });
    expect(view.statusText).toBe('完了（データなし）');
  });
});

describe('Panel 7 opportunity map', () => {
  it('charts the lowest scores first with the category colors', () => {
    const view = renderOpportunityMap(fx.opportunity);
    mount(view);
    expect(view.statusText).toBe('完了（3件）');
    const option = charts.byId['rd-chart-opportunity'] as {
      yAxis: { data: string[] };
      series: { data: { value: number; itemStyle: { color: string } }[] }[];
    };
    expect(option.yAxis.data).toEqual(['穴場区', '標準区', '激戦区']);
    expect(option.series[0]?.data.map((d) => d.value)).toEqual([0.05, 1, 9.5]);
    expect(option.series[0]?.data.map((d) => d.itemStyle.color)).toEqual(['#22c55e', '#64748b', '#ef4444']);
  });

  it('keeps only the top 25 in the chart but counts all municipalities in the status', () => {
    const many = Array.from({ length: 30 }, (_, i) => ({
      name: `m${String(i)}`,
      citycode: i,
      hw_count: 1,
      population: 1,
      score: 30 - i,
      category: '標準',
    }));
    const view = renderOpportunityMap({ ...fx.opportunity, municipalities: many });
    mount(view);
    expect(view.statusText).toBe('完了（30件）');
    const option = charts.byId['rd-chart-opportunity'] as { yAxis: { data: string[] } };
    expect(option.yAxis.data).toHaveLength(25);
    expect(option.yAxis.data[0]).toBe('m29');
  });

  it('shows the empty message with 完了（0件）', () => {
    const view = renderOpportunityMap({ ...fx.opportunity, municipalities: [] });
    expect(view.statusText).toBe('完了（0件）');
  });
});

describe('Panel 8 insights', () => {
  it('colors cards by severity_rank (the API sends Japanese severity names)', () => {
    const view = renderInsights(fx.insights);
    mount(view);
    expect(view.statusText).toBe('完了（4件）');
    const rows = screen.getAllByTestId('rd-insights-row');
    expect(rows).toHaveLength(4);
    expect(rows[0]?.className).toContain('border-red-500');
    expect(rows[1]?.className).toContain('border-orange-500');
    expect(rows[2]?.className).toContain('border-blue-500');
    expect(rows[3]?.className).toContain('border-green-500');
    expect(severityClass(9)).toBe('border-slate-500 bg-navy-900/40');
  });

  it('shows pattern id, title, message and the action only when present', () => {
    mount(renderInsights(fx.insights));
    expect(texts('rd-insights-pattern_id')).toEqual(['P0', 'P1', 'P2', 'P3']);
    expect(texts('rd-insights-title')[0]).toBe('title-P0');
    expect(texts('rd-insights-message')[3]).toBe('message-P3');
    // P3 has an empty hr_action
    expect(texts('rd-insights-hr_action')).toEqual(['👉 action-P0', '👉 action-P1', '👉 action-P2']);
  });
});

describe('Panel 9 talent pool expansion', () => {
  it('shows tier boxes, breakdown rows and the API notes', () => {
    const view = renderTalentPoolExpansion(fx.expansion);
    mount(view);
    expect(view.statusText).toBe('完了（2 市区町村）');
    expect(text('rd-talent_pool_expansion-current')).toBe('東京都 新宿区');
    expect(text('rd-talent_pool_expansion-tier_30min-municipality_count')).toBe('2');
    expect(text('rd-talent_pool_expansion-tier_30min-unemployment_pool')).toBe('+12,345 人');
    expect(text('rd-talent_pool_expansion-tier_30min-hw_postings')).toBe('+678 件');
    expect(text('rd-talent_pool_expansion-tier_60min-unemployment_pool')).toBe('+0 人');
    const rows = screen.getAllByTestId('rd-talent_pool_expansion-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]?.textContent).toBe('東京都 渋谷区5,00011122');
    expect(texts('rd-talent_pool_expansion-breakdown-commuters')).toEqual(['5,000', '4,000']);
    expect(text('rd-talent_pool_expansion-notes-data_source_od')).toBe('※ OD(API)');
    expect(text('rd-talent_pool_expansion-notes-hw_scope')).toBe('※ HW のみ(API)');
  });

  it('falls back to the default note wording when a note is empty', () => {
    mount(
      renderTalentPoolExpansion({
        ...fx.expansion,
        notes: { ...fx.expansion.notes, data_source_od: '', hw_scope: '' },
      }),
    );
    expect(text('rd-talent_pool_expansion-notes-data_source_od')).toBe(
      '※ 通勤 OD は国勢調査 2020 年ベース (5 年遅れ)',
    );
    expect(text('rd-talent_pool_expansion-notes-hw_scope')).toBe('※ HW 求人は HW 掲載のみ');
  });

  it('shows データなし when the commute OD data is not available', () => {
    const unavailable: RdTalentPoolExpansionResponse = { ...fx.expansion, is_data_available: false };
    const view = renderTalentPoolExpansion(unavailable);
    expect(view.statusText).toBe('データなし');
    mount(view);
    expect(document.body.textContent).toContain('通勤 OD データが当該市区町村に対して未投入のため');
  });
});
