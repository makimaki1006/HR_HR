// @vitest-environment happy-dom
// Values at the edge: null (serde writes NaN / Infinity as null), -0, tiny negatives, huge numbers.
// No visible text may be "NaN", "undefined", "Infinity" or "-0", and a missing value must not
// turn into a plausible number (null * 100 = 0).
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const charts = vi.hoisted((): { byId: Record<string, unknown> } => ({ byId: {} }));
vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string; option: unknown }) => {
    charts.byId[props.testId] = props.option;
    return <div data-testid={props.testId} />;
  },
}));

import { opportunityChartOption } from './charts';
import * as fx from './fixtures';
import { fmt, signPrefix } from './format';
import {
  renderDifficulty,
  renderMarketTrend,
  renderOpportunityMap,
  renderTalentPool,
  renderTalentPoolExpansion,
  type PanelView,
} from './panels';

afterEach(() => {
  cleanup();
  charts.byId = {};
});

const nul = null as unknown as number;
const mount = (v: PanelView): string => {
  const { container } = render(<div>{v.body}</div>);
  return container.textContent;
};
const BAD = /NaN|undefined|Infinity|-0(?![.\d])|\+—/;

describe('fmt / signPrefix', () => {
  it('never prints -0 and maps non-finite numbers to the dash', () => {
    expect(fmt(-0)).toBe('0');
    expect(fmt(-0.4)).toBe('0');
    expect(fmt(-0.004, 2)).toBe('0.00');
    expect(fmt(-1)).toBe('-1');
    expect(fmt(Infinity)).toBe('—');
    expect(fmt(-Infinity)).toBe('—');
    expect(fmt(NaN)).toBe('—');
    expect(fmt(null)).toBe('—');
    expect(fmt(undefined)).toBe('—');
  });

  it('keeps a huge number readable', () => {
    expect(fmt(1e15)).toBe('1,000,000,000,000,000');
    expect(fmt(Number.MAX_SAFE_INTEGER)).toBe('9,007,199,254,740,991');
  });

  it('signPrefix is empty for a missing / non-finite value', () => {
    expect(signPrefix(nul)).toBe('');
    expect(signPrefix(NaN)).toBe('');
    expect(signPrefix(0)).toBe('+');
    expect(signPrefix(-0)).toBe('+');
    expect(signPrefix(-3)).toBe('');
  });
});

describe('panels with edge values', () => {
  it('Panel 1: a missing share is a dash, not 0.00 %', () => {
    const t = mount(renderDifficulty({ ...fx.difficulty, metrics: { ...fx.difficulty.metrics, area_share_of_national: nul } }));
    expect(screen.getByTestId('rd-difficulty-metrics-area_share_of_national').textContent).toBe('—%');
    expect(t).not.toMatch(BAD);
  });

  it('Panel 2: commuter_inflow -0 reads +0, null reads a bare dash', () => {
    mount(renderTalentPool({ ...fx.talentPool, metrics: { ...fx.talentPool.metrics, commuter_inflow: -0 } }));
    expect(screen.getByTestId('rd-talent_pool-metrics-commuter_inflow').textContent).toBe('+0');
    cleanup();
    const t = mount(renderTalentPool({ ...fx.talentPool, metrics: { ...fx.talentPool.metrics, commuter_inflow: nul, day_population: nul } }));
    expect(screen.getByTestId('rd-talent_pool-metrics-commuter_inflow').textContent).toBe('—');
    expect(t).not.toMatch(BAD);
  });

  it('Panel 6: a missing growth rate does not read "+—%"; one data point still renders', () => {
    const view = renderMarketTrend({ ...fx.marketTrend, growth_rate_pct: nul, months: ['2026-06'], counts: [125] });
    const t = mount(view);
    expect(view.status).toBe('done');
    expect(screen.getByTestId('rd-market_trend-growth_rate_pct').textContent).toBe('期間中の変動: —%');
    expect(t).not.toMatch(BAD);
  });

  it('Panel 7: one city renders, and the bar label never prints NaN / -0.00', () => {
    const one = { ...fx.opportunity, municipalities: fx.opportunity.municipalities.slice(0, 1) };
    const view = renderOpportunityMap(one);
    expect(view.statusText).toBe('完了（1件）');
    const option = opportunityChartOption(one) as {
      series: { label: { formatter: (p: { value: number }) => string } }[];
    };
    const label = option.series[0]?.label.formatter;
    expect(label?.({ value: NaN })).toBe('—');
    expect(label?.({ value: -0 })).toBe('0.00');
    expect(label?.({ value: -0.001 })).toBe('0.00');
    expect(label?.({ value: 9.5 })).toBe('9.50');
  });

  it('Panel 9: a missing municipality_count / pool is a dash and the status text has no NaN', () => {
    const e = fx.expansion;
    const view = renderTalentPoolExpansion({
      ...e,
      tier_30min: { ...e.tier_30min, municipality_count: nul, unemployment_pool: nul },
    });
    const t = mount(view);
    expect(view.statusText).not.toMatch(BAD);
    expect(screen.getByTestId('rd-talent_pool_expansion-tier_30min-municipality_count').textContent).toBe('—');
    expect(t).not.toMatch(BAD);
  });
});
