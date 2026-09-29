// Uses the real registry (no mock) to prove the registered pieces can draw the chart shapes
// screens need: horizontal bar, stacked bar, line, scatter. SSR mode: no DOM needed.
import { describe, expect, it } from 'vitest';
import { echarts } from './echartsRegistry';

function render(option: Parameters<ReturnType<typeof echarts.init>['setOption']>[0]) {
  const chart = echarts.init(null, undefined, { renderer: 'svg', ssr: true, width: 400, height: 300 });
  chart.setOption({ animation: false, ...option });
  const svg = chart.renderToSVGString();
  const series = (chart.getOption() as { series: { type: string; data: unknown[] }[] }).series;
  chart.dispose();
  return { svg, series };
}

describe('echartsRegistry chart shapes', () => {
  it('horizontal bar (yAxis category, xAxis value)', () => {
    const { svg, series } = render({
      xAxis: { type: 'value' },
      yAxis: { type: 'category', data: ['東京', '大阪', '愛知'] },
      series: [{ type: 'bar', data: [30, 20, 10] }],
    });
    expect(series[0]?.type).toBe('bar');
    expect(series[0]?.data).toEqual([30, 20, 10]);
    expect(svg.startsWith('<svg')).toBe(true);
    expect(svg).toContain('<path');
  });

  it('stacked bar keeps both series and the stack key', () => {
    const { svg, series } = render({
      xAxis: { type: 'category', data: ['a', 'b'] },
      yAxis: { type: 'value' },
      legend: {},
      series: [
        { type: 'bar', stack: 's', name: 'X', data: [1, 2] },
        { type: 'bar', stack: 's', name: 'Y', data: [3, 4] },
      ],
    });
    expect(series.map((s) => s.type)).toEqual(['bar', 'bar']);
    expect(series.map((s) => s.data)).toEqual([[1, 2], [3, 4]]);
    expect(svg).toContain('<path');
  });

  it('line and scatter draw', () => {
    const line = render({
      xAxis: { type: 'category', data: ['a', 'b', 'c'] },
      yAxis: { type: 'value' },
      series: [{ type: 'line', data: [1, 3, 2] }],
    });
    expect(line.series[0]?.type).toBe('line');
    expect(line.series[0]?.data).toEqual([1, 3, 2]);
    const scatter = render({
      xAxis: { type: 'value' },
      yAxis: { type: 'value' },
      series: [
        {type: 'scatter', data: [[1, 2], [3, 4]] },
      ],
    });
    expect(scatter.series[0]?.type).toBe('scatter');
    expect(scatter.series[0]?.data).toEqual([[1, 2], [3, 4]]);
    expect(scatter.svg).toContain('<path');
  });
});
