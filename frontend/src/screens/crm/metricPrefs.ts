// 成績の帯に出す項目の個人設定と、自分の項目の数字 (ADR-028)。
// 設定の正本は Rust の metric_prefs.rs (保存は監査 Turso、1 人 1 行)。数字は custom_metrics.rs。
import { useCallback, useEffect, useState } from 'react';
import { ApiAbortedError, ApiHttpError, apiGet, apiPut } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CustomMetricDef } from '../../generated/CustomMetricDef';
import type { CustomMetricValue } from '../../generated/CustomMetricValue';
import type { CustomMetricsResponse } from '../../generated/CustomMetricsResponse';
import type { MetricKind } from '../../generated/MetricKind';
import type { MetricPrefs } from '../../generated/MetricPrefs';
import type { MetricPrefsResponse } from '../../generated/MetricPrefsResponse';
import { compareTexts, DEFAULT_ITEMS, FIXTURE_AS_OF } from './metricsModel';
import type { StripItem } from './metricsModel';
import type { MetricsFetch } from './useTodayMetrics';

export const MAX_CUSTOM = 6;
/** 「誰の」に Search の条件を 1 つ使うので、自分で足せる条件は 5 つ (Rust の MAX_CUSTOM_CONDITIONS) */
export const MAX_CUSTOM_CONDITIONS = 5;
export const MAX_NAME_CHARS = 12;
export const DEFAULT_OWNER_PROPERTY = 'hubspot_owner_id';
export const KIND_LABELS: Record<MetricKind, string> = { count: '件数', sum: '合計', avg: '平均' };
export const UNAVAILABLE_NOTE = 'いまは項目の設定を読み書きできません。帯は標準の項目で表示しています。';

export const DEFAULT_PREFS: MetricPrefs = { items: [...DEFAULT_ITEMS], custom: [] };

export function newCustomId(): string {
  return `c-${Math.random().toString(36).slice(2, 10).replace(/[^a-z0-9]/g, '') || 'x'}`;
}

/** 自分の項目の数字の書き方 (桁区切り。件数の値は整数、平均・合計は小数 1 位まで) */
export function formatMetricNumber(n: number | null, integer = false): string {
  if (n === null) return '-';
  return new Intl.NumberFormat('ja-JP', { maximumFractionDigits: integer ? 0 : 1 }).format(n);
}

/** 自分の項目 1 つを、もともとの項目と同じ形にする */
export function customStripItem(v: CustomMetricValue, res: Pick<CustomMetricsResponse, 'population'>): StripItem {
  const avgLabel = res.population === 'team' ? 'チーム平均' : '全体平均';
  const base = { key: v.id, label: v.name, extra: null, stale: v.stale };
  if (v.status !== 'ready') {
    return { ...base, value: '', compare: '', compareShort: '', note: v.message ?? (v.status === 'loading' ? '集計しています…' : 'いまは数字を読めません') };
  }
  const value = formatMetricNumber(v.value, v.kind === 'count');
  return { ...base, value, ...compareTexts(avgLabel, formatMetricNumber(v.team_avg), v.rank, v.ranked), note: null };
}

/** 集計がまだ来ていない自分の項目 */
export function pendingCustomItem(d: CustomMetricDef): StripItem {
  return { key: d.id, label: d.name, value: '', compare: '', compareShort: '', extra: null, note: '集計しています…' };
}

/** 自分の項目の数字の状態: まだ来ていない (null)・読めなかった ('failed')・応答 */
export type CustomState = CustomMetricsResponse | null | 'failed';

/** 帯の並び: 設定の `items` の順に、もともとの項目と自分の項目を差し込む */
export function arrangeStrip(builtin: readonly StripItem[], prefs: MetricPrefs, custom: CustomState): StripItem[] {
  const out: StripItem[] = [];
  for (const key of prefs.items) {
    const b = builtin.find(i => i.key === key);
    if (b) { out.push(b); continue; }
    const d = prefs.custom.find(c => c.id === key);
    if (!d) continue;
    if (custom === 'failed') { out.push({ ...pendingCustomItem(d), note: 'いまは数字を読めません' }); continue; }
    const v = custom?.items.find(i => i.id === key);
    if (v && custom) out.push(customStripItem(v, custom));
    // 応答はあるのに項目が無い = 担当者と結び付けられない等 (理由は応答の message)
    else if (custom?.message) out.push({ ...pendingCustomItem(d), note: '出せません' });
    else out.push(pendingCustomItem(d));
  }
  return out;
}

// --- 設定の読み書き -----------------------------------------------------------

export interface PrefsApi {
  load: (signal: AbortSignal) => Promise<ApiResult<MetricPrefsResponse>>;
  save: (prefs: MetricPrefs) => Promise<ApiResult<MetricPrefsResponse>>;
}

export const livePrefsApi: PrefsApi = {
  load: signal => apiGet<MetricPrefsResponse>('/api/crm/metrics/prefs', { signal, timeoutMs: 30_000 }),
  save: prefs => apiPut<MetricPrefsResponse>('/api/crm/metrics/prefs', prefs, { timeoutMs: 35_000 }),
};

/** 手元だけで持つ設定 (架空サンプル・テスト)。保存は同じ入れ物に書く */
export function memoryPrefsApi(initial: MetricPrefsResponse): PrefsApi {
  let current = initial;
  return {
    load: async signal => {
      await Promise.resolve();
      return signal.aborted ? { ok: false, error: new ApiAbortedError('aborted') } : { ok: true, data: current };
    },
    save: async prefs => {
      await Promise.resolve();
      current = { prefs, saved: true, available: true, message: null };
      return { ok: true, data: current };
    },
  };
}

/** 架空サンプルの自分の項目 (URL に `metrics=custom` を付けたとき) */
export const FIXTURE_CUSTOM: CustomMetricDef[] = [
  { id: 'c-absent', name: '不在', kind: 'count', conditions: [{ property: 'bpo_20', operator: 'TODAY', values: [] }, { property: 'dealstage', operator: 'IN', values: ['1095387445'] }], owner_property: 'hubspot_owner_id' },
  { id: 'c-amount', name: '今月の金額', kind: 'sum', property: 'amount', conditions: [{ property: 'bpo_20', operator: 'THIS_MONTH', values: [] }], owner_property: 'hubspot_owner_id' },
];

let fixtureApi: PrefsApi | null = null;
/** 架空サンプルの設定 (画面を開いている間だけ覚える。HubSpot にも Turso にも書かない) */
export function fixturePrefsApi(): PrefsApi {
  if (fixtureApi === null) {
    const seeded = typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('metrics') === 'custom';
    const prefs: MetricPrefs = seeded ? { items: ['outbound', 'over_90s', 'duration', 'appointments', 'c-absent', 'c-amount'], custom: FIXTURE_CUSTOM } : DEFAULT_PREFS;
    fixtureApi = memoryPrefsApi({ prefs, saved: seeded, available: true, message: null });
  }
  return fixtureApi;
}

/** テスト用: 架空サンプルの設定を最初に戻す */
export function resetFixturePrefs(): void { fixtureApi = null; }

export type PrefsState =
  | { phase: 'loading' }
  | { phase: 'ready'; prefs: MetricPrefs; saved: boolean; available: boolean; message: string | null };

export type SaveResult = { ok: true } | { ok: false; message: string };

function saveError(error: unknown): string {
  if (error instanceof ApiHttpError && error.status === 400) {
    const body = error.body as { message?: unknown } | undefined;
    if (typeof body?.message === 'string') return body.message;
  }
  if (error instanceof ApiHttpError && error.status === 503) return '設定を保存できませんでした。いまは項目の設定を読み書きできません。';
  return '設定を保存できませんでした。もう一度保存してください。';
}

/** 本人の設定。読めないときは既定で帯を出し、設定のパネルに案内を出す */
export function useMetricPrefs(api: PrefsApi) {
  const [state, setState] = useState<PrefsState>({ phase: 'loading' });
  useEffect(() => {
    const ctl = new AbortController();
    void api.load(ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      if (r.ok) setState({ phase: 'ready', ...r.data });
      else if (!(r.error instanceof ApiAbortedError)) setState({ phase: 'ready', prefs: DEFAULT_PREFS, saved: false, available: false, message: UNAVAILABLE_NOTE });
    });
    return () => { ctl.abort(); };
  }, [api]);
  const save = useCallback(async (prefs: MetricPrefs): Promise<SaveResult> => {
    const r = await api.save(prefs);
    if (!r.ok) return { ok: false, message: saveError(r.error) };
    setState({ phase: 'ready', ...r.data });
    return { ok: true };
  }, [api]);
  return { state, save };
}

// --- 自分の項目の数字 -------------------------------------------------------------

export const liveCustomFetch: MetricsFetch<CustomMetricsResponse> = signal =>
  apiGet<CustomMetricsResponse>('/api/crm/metrics/custom', { signal, timeoutMs: 30_000 });

function seed(s: string): number {
  let h = 7;
  for (const ch of s) h = (h * 31 + ch.charCodeAt(0)) % 100_003;
  return h;
}

/** 架空サンプルの数字 (定義から決まる架空の値。HubSpot は読まない) */
export function fixtureCustomResponse(prefs: MetricPrefs): CustomMetricsResponse {
  const items = prefs.custom.filter(c => prefs.items.includes(c.id)).map((c): CustomMetricValue => {
    const h = seed(JSON.stringify([c.kind, c.property ?? '', c.conditions, c.owner_property]));
    const count = 3 + (h % 37);
    const value = c.kind === 'count' ? count : c.kind === 'sum' ? count * 25_000 + (h % 9) * 10_000 : 18_000 + (h % 50) * 1_000;
    const team = Math.round(value * (0.7 + (h % 5) / 10) * 10) / 10;
    return { id: c.id, name: c.name, kind: c.kind, status: 'ready', value, team_avg: team, rank: 1 + (h % 8), ranked: 8, stale: false, as_of: FIXTURE_AS_OF, message: null };
  });
  return { pending: false, as_of: FIXTURE_AS_OF, population: 'team', population_size: 8, team_names: ['架空BPOチーム'], items, message: null };
}

export function fixtureCustomFetch(prefs: MetricPrefs): MetricsFetch<CustomMetricsResponse> {
  return async signal => {
    await Promise.resolve();
    return signal.aborted ? { ok: false, error: new ApiAbortedError('aborted') } : { ok: true, data: fixtureCustomResponse(prefs) };
  };
}

/** 背景で読み直している間は、少し後に取り直す */
export const customPending = (d: CustomMetricsResponse): boolean => d.pending;
