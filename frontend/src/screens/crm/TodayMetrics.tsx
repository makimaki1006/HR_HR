import { useState } from 'react';
import { asOfLabel, clockJst, populationLabel, stripItems } from './metricsModel';
import { fixtureTodayFetch, liveTodayFetch, useMetrics } from './useTodayMetrics';
import type { MetricsFetch } from './useTodayMetrics';
import type { TodayMetricsResponse } from '../../generated/TodayMetricsResponse';
import './metrics.css';

const KEY = 'hrhr.crm.todayMetrics.collapsed';
function readCollapsed(): boolean {
  try { return window.localStorage.getItem(KEY) === '1'; } catch { return false; }
}
function writeCollapsed(v: boolean) {
  try { window.localStorage.setItem(KEY, v ? '1' : '0'); } catch { /* 保存できなくても画面は動く */ }
}

export function TodayMetricsView({ data, refreshFailed, collapsed, onToggle }: {
  data: TodayMetricsResponse; refreshFailed: boolean; collapsed: boolean; onToggle: () => void;
}) {
  const items = stripItems(data);
  const first = clockJst(data.me?.metrics.first_call_at ?? null);
  const last = clockJst(data.me?.metrics.last_call_at ?? null);
  const detail = `${populationLabel(data)}と比べています。${first !== null && last !== null ? `最初の架電 ${first}・最後の架電 ${last}。` : ''}${data.me?.rank.outbound === null ? '今日はまだ架電がないため、順位は出ません。' : ''}`;
  return <section className="tm" aria-label="今日の架電成績" data-testid="today-metrics">
    <div className="tm-head">
      <strong title={detail}>今日の成績</strong>
      <span className="tm-asof" title={asOfLabel(data.as_of)} data-testid="metrics-as-of">{asOfLabel(data.as_of)}</span>
      {!collapsed && items !== null && <ul className="tm-items">
        {items.map(it => <li key={it.key} className="tm-item" data-testid={`metric-${it.key}`} title={`${it.label} ${it.value}（${it.compare}）${it.extra ?? ''}`}>
          <span className="tm-label">{it.label}</span>
          <span className="tm-value">{it.value}</span>
          <span className="tm-cmp" data-short={`（${it.compareShort}）`}>（{it.compare}）</span>
          {it.extra !== null && <span className="tm-sub">{it.extra}</span>}
        </li>)}
      </ul>}
      {(data.stale || refreshFailed) && <span className="tm-warn" role="status">最新の数字を読めていません（前回の数字を表示中）</span>}
      <button type="button" className="tm-toggle" aria-expanded={!collapsed} onClick={onToggle}>{collapsed ? '開く' : '閉じる'}</button>
    </div>
    {!collapsed && items === null && <p className="tm-note" role="status">{data.message ?? '個人の成績を出せません。'}</p>}
  </section>;
}

/** 架電画面の上に出す今日の成績の帯。2 分ごとに自動で更新する。fixture のときは架空の数字 */
export function TodayMetrics({ fixture, fetcher }: { fixture: boolean; fetcher?: MetricsFetch<TodayMetricsResponse> }) {
  const f = fetcher ?? (fixture ? fixtureTodayFetch : liveTodayFetch);
  const { state, reload } = useMetrics(f);
  const [collapsed, setCollapsed] = useState(readCollapsed);
  if (state.phase === 'loading') return <section className="tm" aria-label="今日の架電成績"><div className="tm-head"><strong>今日の成績</strong><span className="tm-asof">読み込んでいます</span></div></section>;
  if (state.phase === 'warming') {
    return <section className="tm" aria-label="今日の架電成績"><div className="tm-head"><strong>今日の成績</strong>
      <span className="tm-asof" role="status" data-testid="metrics-warming">今日の成績を集計しています…</span></div></section>;
  }
  if (state.phase === 'error') {
    return <section className="tm" aria-label="今日の架電成績"><div className="tm-head"><strong>今日の成績</strong>
      <span className="tm-warn" role="status">{state.message}</span>
      <button type="button" className="tm-toggle" onClick={reload}>再読み込み</button></div></section>;
  }
  return <TodayMetricsView data={state.data} refreshFailed={state.refreshFailed} collapsed={collapsed}
    onToggle={() => { const next = !collapsed; setCollapsed(next); writeCollapsed(next); }} />;
}
