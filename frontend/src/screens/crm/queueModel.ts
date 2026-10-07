import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueueScope } from '../../generated/CallQueueScope';

/** 架電キューのステージ (Rust `call_queue.rs` の許可ステージと同じ。ID は HubSpot のステージ ID)。 */
export const QUEUE_STAGES: readonly { id: string; label: string }[] = [
  { id: '1095387442', label: '未済' },
  { id: '1095387443', label: '不通' },
  { id: '1095387444', label: '受付ブロック' },
  { id: '1095387445', label: '不在' },
  { id: '1274330477', label: '番号検索依頼中' },
  { id: '1095387446', label: '担当者ブロック' },
  { id: '1409897995', label: '成果報酬のみ' },
  { id: '1095387447', label: 'ニーズなし/無料のみ' },
  { id: '1325087323', label: 'ニーズなし/有料あり' },
  { id: '1325087324', label: 'ニーズあり/無料のみ' },
  { id: '1095387448', label: 'ニーズあり/有料あり' },
  { id: '1448079987', label: 'SV依頼案件' },
  { id: '1319310149', label: '日程確保' },
  { id: '1095457877', label: '案件差戻' },
  { id: '1330563334', label: '商談未実施処理' },
  { id: '1369739056', label: 'リスト精査前' },
];
export const QUEUE_STAGE_IDS: readonly string[] = QUEUE_STAGES.map(s => s.id);

export const QUEUE_SORTS = [
  { value: 'default', label: '標準(次回日が来たもの → 未済)' },
  { value: 'next_call_asc', label: '次回架電日が古い順' },
  { value: 'next_call_desc', label: '次回架電日が新しい順' },
  { value: 'last_call_asc', label: '最終架電日が古い順(未架電が先頭)' },
  { value: 'last_call_desc', label: '最終架電日が新しい順' },
] as const;
export type QueueSort = (typeof QUEUE_SORTS)[number]['value'];
const SORT_VALUES: readonly string[] = QUEUE_SORTS.map(s => s.value);

export type QueueDue = 'all' | 'today';
export type QueueMode = 'live' | 'fixture';

export interface QueueFilters {
  q: string;
  /** 空 = すべてのステージ */
  stages: string[];
  /** '' = 既定 (管理者は全員、それ以外は自分)。'all' | 'me' | 'unassigned' | HubSpot owner ID */
  owner: string;
  due: QueueDue;
  sort: QueueSort;
  nextFrom: string;
  nextTo: string;
  lastFrom: string;
  lastTo: string;
}

export const DEFAULT_FILTERS: QueueFilters = {
  q: '', stages: [], owner: '', due: 'all', sort: 'default',
  nextFrom: '', nextTo: '', lastFrom: '', lastTo: '',
};

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
/** 実在する日付で、サーバ (2000〜2100 年) が受け付ける範囲か */
export function isValidDate(value: string): boolean {
  if (!DATE_RE.test(value)) return false;
  const [y, m, d] = value.split('-').map(Number) as [number, number, number];
  if (y < 2000 || y > 2100) return false;
  const t = new Date(Date.UTC(y, m - 1, d));
  return t.getUTCFullYear() === y && t.getUTCMonth() === m - 1 && t.getUTCDate() === d;
}

export function isValidOwner(value: string): boolean {
  return value === '' || value === 'all' || value === 'me' || value === 'unassigned' || /^\d{1,20}$/.test(value);
}

/** 画面の入力の誤り。空なら取得してよい */
export function validateFilters(f: QueueFilters): string[] {
  const errors: string[] = [];
  const check = (label: string, from: string, to: string) => {
    if (from && !isValidDate(from)) errors.push(`${label}の開始日が正しい日付ではありません`);
    if (to && !isValidDate(to)) errors.push(`${label}の終了日が正しい日付ではありません`);
    if (from && to && isValidDate(from) && isValidDate(to) && from > to) {
      errors.push(`${label}の開始日が終了日より後になっています`);
    }
  };
  check('次回架電日', f.nextFrom, f.nextTo);
  check('最終架電日', f.lastFrom, f.lastTo);
  if (!isValidOwner(f.owner)) errors.push('担当者の指定が正しくありません');
  if (f.q.trim().length > 100) errors.push('キーワードは 100 文字までです');
  return errors;
}

/** URL の検索文字列 → 条件。不正な値は既定値に落とす (画面を壊さない) */
export function parseFilters(search: string): QueueFilters {
  const p = new URLSearchParams(search);
  const stageSet = new Set(p.getAll('stage').filter(s => QUEUE_STAGE_IDS.includes(s)));
  const sort = p.get('sort') ?? '';
  const owner = p.get('owner') ?? '';
  const date = (k: string) => {
    const v = p.get(k) ?? '';
    return isValidDate(v) ? v : '';
  };
  return {
    q: (p.get('q') ?? '').slice(0, 100),
    stages: QUEUE_STAGE_IDS.filter(id => stageSet.has(id)),
    owner: isValidOwner(owner) ? owner : '',
    due: p.get('due') === 'today' ? 'today' : 'all',
    sort: SORT_VALUES.includes(sort) ? (sort as QueueSort) : 'default',
    nextFrom: date('next_from'), nextTo: date('next_to'),
    lastFrom: date('last_from'), lastTo: date('last_to'),
  };
}

export function parseMode(search: string): QueueMode {
  return new URLSearchParams(search).get('mode') === 'fixture' ? 'fixture' : 'live';
}

/** 既定と違う条件だけを並べた検索文字列 (API と URL の共通部分。cursor / limit は含めない) */
export function filtersToParams(f: QueueFilters): URLSearchParams {
  const p = new URLSearchParams();
  if (f.q.trim()) p.set('q', f.q.trim());
  for (const s of f.stages) p.append('stage', s);
  if (f.owner) p.set('owner', f.owner);
  if (f.due !== 'all') p.set('due', f.due);
  if (f.sort !== 'default') p.set('sort', f.sort);
  if (f.nextFrom) p.set('next_from', f.nextFrom);
  if (f.nextTo) p.set('next_to', f.nextTo);
  if (f.lastFrom) p.set('last_from', f.lastFrom);
  if (f.lastTo) p.set('last_to', f.lastTo);
  return p;
}

export const QUEUE_PAGE_SIZE = 25;

/** `GET /api/crm/call-queue` のパス (cursor は同じ条件に対してだけ付ける) */
export function queueApiPath(f: QueueFilters, cursor: string | null): string {
  const p = filtersToParams(f);
  p.set('limit', String(QUEUE_PAGE_SIZE));
  if (cursor) p.set('cursor', cursor);
  return `/api/crm/call-queue?${p.toString()}`;
}

/** 画面の URL (`?view=queue` + 条件 + モード) */
export function screenSearch(f: QueueFilters, mode: QueueMode): string {
  const p = new URLSearchParams({ view: 'queue' });
  if (mode === 'fixture') p.set('mode', 'fixture');
  for (const [k, v] of filtersToParams(f)) p.append(k, v);
  return `?${p.toString()}`;
}

/** 条件が同じかの比較用キー (取得のやり直し判定) */
export function filtersKey(f: QueueFilters): string {
  return filtersToParams(f).toString();
}

/**
 * 応答の `scope` が、いま画面にある条件と一致するか。一致しない応答は表示に使わない。
 * owner は、画面が既定 ('') のときサーバの既定 (管理者=all / それ以外=me) を受け入れる。
 */
export function scopeMatches(scope: CallQueueScope, f: QueueFilters): boolean {
  const wantStages = [...(f.stages.length ? f.stages : QUEUE_STAGE_IDS)].sort();
  const gotStages = [...scope.stages].sort();
  const ownerOk = f.owner === '' ? true : scope.owner === f.owner;
  return ownerOk
    && scope.due === f.due
    && scope.sort === f.sort
    && (scope.q ?? '') === f.q.trim()
    && wantStages.length === gotStages.length && wantStages.every((s, i) => s === gotStages[i])
    && (scope.next_from ?? '') === f.nextFrom && (scope.next_to ?? '') === f.nextTo
    && (scope.last_from ?? '') === f.lastFrom && (scope.last_to ?? '') === f.lastTo;
}

/** 同じ deal_id を 2 回出さずに追記する (先に出た行を残す)。ページ跨ぎの更新で重複しうる */
export function mergeItems(existing: readonly CallQueueItem[], incoming: readonly CallQueueItem[]): CallQueueItem[] {
  const seen = new Set(existing.map(i => i.deal_id));
  const out = [...existing];
  for (const item of incoming) {
    if (seen.has(item.deal_id)) continue;
    seen.add(item.deal_id);
    out.push(item);
  }
  return out;
}

/** error_kind ごとの文言 */
export function errorMessage(kind: string | null, status: number | null): string {
  switch (kind) {
    case 'hubspot_rate_limited': return 'HubSpot の呼び出し回数の上限に達しました。少し待ってから再試行してください。';
    case 'hubspot_timeout': return 'HubSpot からの応答が時間内に返りませんでした。再試行してください。';
    case 'crm_timeout': return '取得に時間がかかりすぎたため中断しました。条件を絞って再試行してください。';
    case 'hubspot_auth': return 'HubSpot への接続設定(認証)に問題があります。管理者に連絡してください。';
    case 'hubspot_upstream':
    case 'hubspot_transport': return 'HubSpot との通信に失敗しました。再試行してください。';
    case 'hubspot_decode': return 'HubSpot の応答を読み取れませんでした。管理者に連絡してください。';
    case 'not_configured': return 'HubSpot への接続が設定されていません。管理者に連絡してください。';
    case 'cursor_mismatch': return '続きの読み込みに使う情報が古くなりました。最初から読み直してください。';
    case 'invalid_param': return '条件の指定が正しくないため取得できませんでした。条件を見直してください。';
    case 'owner_not_resolved': return 'あなたのメールアドレスに対応する HubSpot の所有者が見つかりません。所有者を選んでください。';
    default: return status === null
      ? 'ネットワークに接続できませんでした。接続を確認して再試行してください。'
      : `取得に失敗しました(${String(status)})。再試行してください。`;
  }
}

export function unauthorizedMessage(kind: string | null, status: number): string {
  if (status === 401 || kind === 'auth_required') return 'ログインが必要です。Google Workspace でログインし直してください。';
  return 'このアカウントには架電キューを見る権限がありません。';
}

/** HubSpot の日付値 (`YYYY-MM-DD` または UTC 0 時のエポック ms) を `YYYY-MM-DD` にする。読めなければ null */
export function dateValue(raw: string | null): string | null {
  if (!raw) return null;
  const t = raw.trim();
  if (DATE_RE.test(t)) return t.slice(0, 10);
  if (/^\d{12,13}$/.test(t)) return new Date(Number(t)).toISOString().slice(0, 10);
  const m = /^(\d{4}-\d{2}-\d{2})T/.exec(t);
  return m?.[1] ?? null;
}
