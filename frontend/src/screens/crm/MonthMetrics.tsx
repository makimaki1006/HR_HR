import type { ReactNode } from 'react';
import type { MonthMetricsResponse } from '../../generated/MonthMetricsResponse';
import { asOfLabel, monthName, monthStripItems } from './metricsModel';
import { fixtureMonthFetch, liveMonthFetch, useMetrics } from './useTodayMetrics';
import type { MetricsFetch } from './useTodayMetrics';
import './metrics.css';

/** 新規営業の人の帯。今月の数字を主にし、今日の架電数は小さく添える */
export function MonthMetricsView({ data, refreshFailed, collapsed, onToggle }: {
  data: MonthMetricsResponse; refreshFailed: boolean; collapsed: boolean; onToggle: () => void;
}) {
  const items = monthStripItems(data);
  const title = `${monthName(data.month)}の成績`;
  const detail = `新規営業のチームで今月架電した ${String(data.population_size)} 人と比べています。アポは「アポ日確定」に今月入った取引の担当者に付きます。`;
  return <section className="tm" aria-label="今月の架電成績" data-testid="month-metrics">
    <div className="tm-head">
      <strong title={detail}>{title}</strong>
      <span className="tm-asof" title={asOfLabel(data.as_of)} data-testid="metrics-as-of">{asOfLabel(data.as_of)}</span>
      {!collapsed && items !== null && <ul className="tm-items">
        {items.map(it => <li key={it.key} className="tm-item" data-testid={`metric-${it.key}`} title={`${it.label} ${it.value}（${it.compare}）${it.extra ?? ''}`}>
          <span className="tm-label">{it.label}</span>
          <span className="tm-value">{it.value}</span>
          <span className="tm-cmp" data-short={`（${it.compareShort}）`}>（{it.compare}）</span>
          {it.extra !== null && <span className="tm-sub" data-testid="metric-today">{it.extra}</span>}
        </li>)}
      </ul>}
      {(data.stale || refreshFailed) && <span className="tm-warn" role="status">最新の数字を読めていません（前回の数字を表示中）</span>}
      <button type="button" className="tm-toggle" aria-expanded={!collapsed} onClick={onToggle}>{collapsed ? '開く' : '閉じる'}</button>
    </div>
    {!collapsed && items === null && <p className="tm-note" role="status">{data.message ?? '個人の成績を出せません。'}</p>}
  </section>;
}

/**
 * 新規営業の人の今月の帯。数字が来るまで (集計中・読み込めない) は `fallback` (今日の帯) を出す。
 * 今月の読み込みは背景で進むので、集計中のあいだも今日の成績は見える。
 */
export function MonthMetrics({ fixture, fetcher, fallback, collapsed, onToggle }: {
  fixture: boolean; fetcher?: MetricsFetch<MonthMetricsResponse> | undefined; fallback: ReactNode; collapsed: boolean; onToggle: () => void;
}) {
  const f = fetcher ?? (fixture ? fixtureMonthFetch : liveMonthFetch);
  const { state } = useMetrics(f);
  if (state.phase === 'warming') {
    return <>
      {fallback}
      <p className="tm-note" role="status" data-testid="month-warming">今月の成績を集計しています…</p>
    </>;
  }
  if (state.phase !== 'ready') return <>{fallback}</>;
  return <MonthMetricsView data={state.data} refreshFailed={state.refreshFailed} collapsed={collapsed} onToggle={onToggle} />;
}
