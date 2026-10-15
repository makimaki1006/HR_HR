import type { ReactNode } from 'react';
import type { MonthMetricsResponse } from '../../generated/MonthMetricsResponse';
import { asOfLabel, monthName, monthStripItems } from './metricsModel';
import { arrangeStrip, DEFAULT_PREFS } from './metricPrefs';
import { MetricsStrip } from './MetricsStrip';
import type { StripExtras } from './TodayMetrics';
import { fixtureMonthFetch, liveMonthFetch, useMetrics } from './useTodayMetrics';
import type { MetricsFetch } from './useTodayMetrics';
import './metrics.css';

const NO_EXTRAS: StripExtras = { prefs: DEFAULT_PREFS, custom: null, onSettings: null, settingsOpen: false, panel: null };

/** 新規営業の人の帯。今月の数字を主にし、今日の架電数は小さく添える。項目の並びと自分の項目は今日の帯と同じ設定 */
export function MonthMetricsView({ data, refreshFailed, collapsed, onToggle, extras: given }: {
  data: MonthMetricsResponse; refreshFailed: boolean; collapsed: boolean; onToggle: () => void; extras?: StripExtras | undefined;
}) {
  const extras = given ?? NO_EXTRAS;
  const builtin = monthStripItems(data, extras.prefs.items);
  const items = builtin === null ? null : arrangeStrip(builtin, extras.prefs, extras.custom);
  const title = `${monthName(data.month)}の成績`;
  const detail = `新規営業のチームで今月架電した ${String(data.population_size)} 人と比べています。アポは「アポ日確定」に今月入った取引の担当者に付きます。自分の項目の期間は、それぞれの条件で決まります。`;
  const stale = data.stale || refreshFailed || (extras.custom !== null && extras.custom !== 'failed' && extras.custom.items.some(i => i.stale));
  return <MetricsStrip label="今月の架電成績" testId="month-metrics" title={title} detail={detail} asOf={asOfLabel(data.as_of)} asOfTitle={asOfLabel(data.as_of)}
    items={items} warn={stale ? '最新の数字を読めていません（前回の数字を表示中）' : null} collapsed={collapsed} onToggle={onToggle}
    onSettings={extras.onSettings} settingsOpen={extras.settingsOpen} note={items === null ? (data.message ?? '個人の成績を出せません。') : null}>
    {extras.panel}
  </MetricsStrip>;
}

/**
 * 新規営業の人の今月の帯。数字が来るまで (集計中・読み込めない) は `fallback` (今日の帯) を出す。
 * 今月の読み込みは背景で進むので、集計中のあいだも今日の成績は見える。
 */
export function MonthMetrics({ fixture, fetcher, fallback, collapsed, onToggle, extras }: {
  fixture: boolean; fetcher?: MetricsFetch<MonthMetricsResponse> | undefined; fallback: ReactNode; collapsed: boolean; onToggle: () => void; extras?: StripExtras;
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
  return <MonthMetricsView data={state.data} refreshFailed={state.refreshFailed} collapsed={collapsed} onToggle={onToggle} extras={extras} />;
}
