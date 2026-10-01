import type { EChartsCoreOption } from 'echarts/core';
import type { RdMarketTrendResponse } from '../../generated/RdMarketTrendResponse';
import type { RdOpportunityMapResponse } from '../../generated/RdOpportunityMapResponse';
import type { RdOpportunityMunicipality } from '../../generated/RdOpportunityMunicipality';
import { fmt } from './format';

// The old page used echarts.init(el, 'dark'); the shared EChart has no theme, so the text colors
// the dark theme provided are set here.
// Intended difference from the old page: the 'dark' theme also painted a dark navy chart
// background; here backgroundColor is 'transparent', so the chart takes the card's background.
const TEXT = '#cbd5e1';
const AXIS_LABEL = '#94a3b8';

export const OPPORTUNITY_TOP_N = 25;

export function opportunityColor(category: string): string {
  if (category === '穴場') return '#22c55e';
  if (category === '激戦') return '#ef4444';
  return '#64748b';
}

/** Score ascending (smallest = best opportunity first), top 25. Missing score counts as 0. */
export function opportunityTop(cities: readonly RdOpportunityMunicipality[]): RdOpportunityMunicipality[] {
  return cities
    .slice()
    .sort((a, b) => a.score - b.score)
    .slice(0, OPPORTUNITY_TOP_N);
}

export function trendChartOption(d: RdMarketTrendResponse): EChartsCoreOption {
  const metricLabel = d.metric_label || (d.is_sample ? '業界サンプル件数' : '月次求人件数');
  return {
    backgroundColor: 'transparent',
    textStyle: { color: TEXT },
    tooltip: { trigger: 'axis' },
    grid: { left: 60, right: 30, top: 20, bottom: 40 },
    xAxis: { type: 'category', data: d.months, axisLabel: { color: AXIS_LABEL, rotate: 30 } },
    yAxis: { type: 'value', axisLabel: { color: AXIS_LABEL }, name: metricLabel },
    series: [
      {
        name: metricLabel,
        type: 'line',
        data: d.counts,
        smooth: true,
        symbol: 'circle',
        itemStyle: { color: d.is_sample ? '#0ea5e9' : '#3b82f6' },
        areaStyle: { color: d.is_sample ? 'rgba(14,165,233,0.1)' : 'rgba(59,130,246,0.1)' },
      },
    ],
  };
}

export function opportunityChartOption(d: RdOpportunityMapResponse): EChartsCoreOption {
  const top = opportunityTop(d.municipalities);
  return {
    backgroundColor: 'transparent',
    textStyle: { color: TEXT },
    tooltip: {
      trigger: 'axis',
      formatter: (params: { dataIndex: number; value: number }[]): string => {
        const p = params[0];
        const c = p ? top[p.dataIndex] : undefined;
        if (!p || !c) return '';
        return (
          `${c.name}<br/>スコア: ${fmt(p.value, 2)}` +
          `<br/>HW求人数: ${fmt(c.hw_count)}` +
          `<br/>昼人口: ${fmt(c.population)}` +
          `<br/>区分: ${c.category || '—'}`
        );
      },
    },
    grid: { left: 160, right: 40, top: 20, bottom: 30 },
    xAxis: { type: 'value', name: 'スコア（千人あたり求人数）', axisLabel: { color: AXIS_LABEL } },
    yAxis: {
      type: 'category',
      data: top.map((c) => c.name),
      axisLabel: { color: TEXT, fontSize: 11 },
    },
    series: [
      {
        type: 'bar',
        data: top.map((c) => ({ value: c.score, itemStyle: { color: opportunityColor(c.category) } })),
        label: {
          show: true,
          position: 'right',
          formatter: (p: { value: number }): string => fmt(p.value, 2),
          color: '#e2e8f0',
        },
      },
    ],
  };
}
