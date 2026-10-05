import type { EChartsCoreOption } from 'echarts/core';
import type { JobCopyRecord } from './data';

export interface MarketData {
  source: string;
  titles: string[];
  prefectures: string[];
  ctk_basis: string;
  series: { prefecture: string; months: string[]; job_count: (number | null)[]; ctk_count: (number | null)[]; employer_count: (number | null)[]; seekers_per_posting: (number | null)[] } | null;
}
export interface MarketRow { month: string; jobs: number | null; viewers: number | null; employers: number | null; viewersPerJob: number | null }

/** Absent report months are missing observations, never zero or interpolated. */
export function marketRows(series: NonNullable<MarketData['series']>): MarketRow[] {
  const rows = new Map<string, MarketRow>();
  const value = (n: number | null | undefined) => typeof n === 'number' && Number.isFinite(n) && n >= 0 ? n : null;
  series.months.forEach((month, index) => {
    if (!/^\d{4}-(0[1-9]|1[0-2])$/.test(month)) return;
    rows.set(month, { month, jobs: value(series.job_count[index]), viewers: value(series.ctk_count[index]), employers: value(series.employer_count[index]), viewersPerJob: value(series.seekers_per_posting[index]) });
  });
  const months = [...rows.keys()].sort();
  const first = months[0]; const last = months.at(-1);
  if (!first || !last) return [];
  const result: MarketRow[] = [];
  let cursor = first;
  // Bounded calendar fill; dates are month keys, not browser-local timestamps.
  while (cursor <= last && result.length < 1200) {
    result.push(rows.get(cursor) ?? { month: cursor, jobs: null, viewers: null, employers: null, viewersPerJob: null });
    const year = Number(cursor.slice(0, 4)); const month = Number(cursor.slice(5));
    cursor = `${String(month === 12 ? year + 1 : year).padStart(4, '0')}-${String(month === 12 ? 1 : month + 1).padStart(2, '0')}`;
  }
  return result;
}

/** Counts only supplied application dates; no allocation to copy versions. */
export function monthlyApplications(job: JobCopyRecord): { month: string; count: number | null }[] | null {
  const dates = job.overallApplications?.byDate;
  if (!dates) return null;
  const months = new Map<string, number>();
  Object.entries(dates).forEach(([date, count]) => {
    const month = date.slice(0, 7);
    months.set(month, (months.get(month) ?? 0) + count);
  });
  const keys = [...months.keys()].sort();
  return marketRows({ prefecture: '', months: keys, job_count: keys.map(key => months.get(key) ?? null), ctk_count: [], employer_count: [], seekers_per_posting: [] }).map(row => ({ month: row.month, count: row.jobs }));
}

export function trendOption(months: string[], values: (number | null)[], label: string, unit: string, color = '#2463a7', bar = false, missingLabel = '未取得'): EChartsCoreOption {
  return {
    animation: false,
    aria: { enabled: true, decal: { show: true }, description: `${label}。横軸は対象月、縦軸は${unit}。未取得は欠損として表示。下の数値表で実数を確認できます。` },
    textStyle: { fontFamily: 'sans-serif', fontSize: 13, color: '#334155' },
    grid: { top: 36, left: 14, right: 18, bottom: months.length > 12 ? 76 : 30, containLabel: true },
    tooltip: { trigger: 'axis', renderMode: 'richText', confine: true, valueFormatter: (value: unknown) => typeof value === 'number' ? `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${unit}` : missingLabel },
    xAxis: { type: 'category', data: months, axisLabel: { hideOverlap: true, showMinLabel: true, showMaxLabel: true } },
    yAxis: { type: 'value', min: 0, ...(unit === '件' || unit === '社' ? { minInterval: 1 } : {}), name: unit, splitLine: { lineStyle: { color: '#e2e8f0' } } },
    ...(months.length > 12 ? { dataZoom: [{ type: 'slider', start: 0, end: 100, bottom: 8, height: 22 }] } : {}),
    series: [{ name: label, type: bar || months.length <= 2 ? 'bar' : 'line', data: values, connectNulls: false, showSymbol: true, symbolSize: 7, barMaxWidth: 48, itemStyle: { color }, lineStyle: { color, width: 2 } }],
  };
}
