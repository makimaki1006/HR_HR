import { useCallback, useState } from 'react';
import type { ReactNode } from 'react';
import { asOfLabel, clockJst, populationLabel, stripItems } from './metricsModel';
import { MonthMetrics } from './MonthMetrics';
import { MetricsStrip } from './MetricsStrip';
import { MetricsSettings } from './MetricsSettings';
import {
  DEFAULT_PREFS, arrangeStrip, customPending, fixtureCustomFetch, fixturePrefsApi, liveCustomFetch, livePrefsApi, memoryPrefsApi, useMetricPrefs,
} from './metricPrefs';
import type { CustomState, PrefsApi } from './metricPrefs';
import { liveTodayFetch, pickFixtureToday, useMetrics } from './useTodayMetrics';
import type { MetricsFetch } from './useTodayMetrics';
import type { CatalogState } from './usePropertyCatalog';
import type { ValueChoiceSource } from './queueValueChoices';
import type { CustomMetricsResponse } from '../../generated/CustomMetricsResponse';
import type { MetricPrefs } from '../../generated/MetricPrefs';
import type { MonthMetricsResponse } from '../../generated/MonthMetricsResponse';
import type { TodayMetricsResponse } from '../../generated/TodayMetricsResponse';
import './metrics.css';

const KEY = 'hrhr.crm.todayMetrics.collapsed';
function readCollapsed(): boolean {
  try { return window.localStorage.getItem(KEY) === '1'; } catch { return false; }
}
function writeCollapsed(v: boolean) {
  try { window.localStorage.setItem(KEY, v ? '1' : '0'); } catch { /* 保存できなくても画面は動く */ }
}

/** 帯の設定と自分の項目 (今日・今月の帯で共有する) */
export interface StripExtras {
  prefs: MetricPrefs;
  custom: CustomState;
  onSettings: (() => void) | null;
  settingsOpen: boolean;
  /** 設定のパネル (開いているときだけ) */
  panel: ReactNode;
}

const NO_EXTRAS: StripExtras = { prefs: DEFAULT_PREFS, custom: null, onSettings: null, settingsOpen: false, panel: null };

export function TodayMetricsView({ data, refreshFailed, collapsed, onToggle, extras = NO_EXTRAS }: {
  data: TodayMetricsResponse; refreshFailed: boolean; collapsed: boolean; onToggle: () => void; extras?: StripExtras;
}) {
  const builtin = stripItems(data, extras.prefs.items);
  const items = builtin === null ? null : arrangeStrip(builtin, extras.prefs, extras.custom);
  const first = clockJst(data.me?.metrics.first_call_at ?? null);
  const last = clockJst(data.me?.metrics.last_call_at ?? null);
  const detail = `${populationLabel(data)}と比べています。${first !== null && last !== null ? `最初の架電 ${first}・最後の架電 ${last}。` : ''}${data.me?.rank.outbound === null ? '今日はまだ架電がないため、順位は出ません。' : ''}`;
  const stale = data.stale || refreshFailed || (extras.custom !== null && extras.custom !== 'failed' && extras.custom.items.some(i => i.stale));
  return <MetricsStrip label="今日の架電成績" testId="today-metrics" title="今日の成績" detail={detail} asOf={asOfLabel(data.as_of)} asOfTitle={asOfLabel(data.as_of)}
    items={items} warn={stale ? '最新の数字を読めていません（前回の数字を表示中）' : null} collapsed={collapsed} onToggle={onToggle}
    onSettings={extras.onSettings} settingsOpen={extras.settingsOpen} note={items === null ? (data.message ?? '個人の成績を出せません。') : null}>
    {extras.panel}
  </MetricsStrip>;
}

/** 設定のパネルに渡す材料 (架電画面が持つ項目の一覧・選べる値) */
export interface MetricsSettingsDeps {
  catalog: CatalogState;
  onReloadCatalog: () => void;
  choices?: ValueChoiceSource;
  /** パネルを開いた・閉じた (項目の一覧を読み始める合図) */
  onOpenChange?: (open: boolean) => void;
}

function showsCustom(p: MetricPrefs): boolean {
  return p.items.some(k => p.custom.some(c => c.id === k));
}

const NO_CUSTOM: MetricsFetch<CustomMetricsResponse> = () => Promise.resolve({ ok: true, data: { pending: false, as_of: '', population: 'all', population_size: 0, team_names: [], items: [], message: null } });

/** 架電画面の上に出す今日の成績の帯。2 分ごとに自動で更新する。fixture のときは架空の数字 */
export function TodayMetrics({ fixture, fetcher, monthFetcher, prefsApi, customFetcher, settings }: {
  fixture: boolean; fetcher?: MetricsFetch<TodayMetricsResponse>; monthFetcher?: MetricsFetch<MonthMetricsResponse> | undefined;
  /** 帯の設定の読み書き (既定: 実データは API、架空サンプルは画面の中だけ) */
  prefsApi?: PrefsApi;
  /** 自分の項目の数字 (テスト用に差し替える) */
  customFetcher?: (prefs: MetricPrefs) => MetricsFetch<CustomMetricsResponse>;
  settings?: MetricsSettingsDeps;
}) {
  const f = fetcher ?? (fixture ? pickFixtureToday() : liveTodayFetch);
  const { state, reload } = useMetrics(f);
  const [collapsed, setCollapsed] = useState(readCollapsed);
  // テストで今日の数字だけ差し替えたときは、設定を手元の既定で持つ (API を呼ばない)
  const [api] = useState<PrefsApi>(() => prefsApi ?? (fixture ? fixturePrefsApi() : fetcher ? memoryPrefsApi({ prefs: DEFAULT_PREFS, saved: false, available: true, message: null }) : livePrefsApi));
  const { state: prefsState, save } = useMetricPrefs(api);
  const prefs = prefsState.phase === 'ready' ? prefsState.prefs : DEFAULT_PREFS;
  const hasCustom = showsCustom(prefs);
  // 設定 (の版) ごとに 1 つの関数。保存したら新しい関数になり、すぐ読み直す
  const [fetchers] = useState(() => new WeakMap<MetricPrefs, MetricsFetch<CustomMetricsResponse>>());
  let customF = fetchers.get(prefs);
  if (customF === undefined) {
    customF = !hasCustom ? NO_CUSTOM : customFetcher ? customFetcher(prefs) : fixture ? fixtureCustomFetch(prefs) : (signal => liveCustomFetch(signal));
    fetchers.set(prefs, customF);
  }
  const { state: customState } = useMetrics(customF, undefined, undefined, customPending);
  const custom: CustomState = !hasCustom ? null : customState.phase === 'ready' ? customState.data : customState.phase === 'error' ? 'failed' : null;
  const [open, setOpen] = useState(false);
  const onOpenChange = settings?.onOpenChange;
  const setPanel = useCallback((v: boolean) => { setOpen(v); onOpenChange?.(v); }, [onOpenChange]);

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
  const onToggle = () => { const next = !collapsed; setCollapsed(next); writeCollapsed(next); };
  const ready = prefsState.phase === 'ready';
  const panel = open && ready && settings
    ? <MetricsSettings prefs={prefsState.prefs} available={prefsState.available} note={prefsState.message} monthView={state.data.month_view}
      catalog={settings.catalog} onReloadCatalog={settings.onReloadCatalog} choices={settings.choices} onSave={save} onClose={() => { setPanel(false); }} />
    : null;
  const extras: StripExtras = { prefs, custom, onSettings: settings && ready ? () => { setPanel(!open); } : null, settingsOpen: open, panel };
  const today = <TodayMetricsView data={state.data} refreshFailed={state.refreshFailed} collapsed={collapsed} onToggle={onToggle} extras={extras} />;
  // 新規営業の人は今月の帯を主にする (BPO は今日のまま)。今月が読めるまでは今日の帯を出す
  if (state.data.month_view) {
    return <MonthMetrics fixture={fixture} fetcher={monthFetcher} fallback={today} collapsed={collapsed} onToggle={onToggle} extras={extras} />;
  }
  return today;
}
