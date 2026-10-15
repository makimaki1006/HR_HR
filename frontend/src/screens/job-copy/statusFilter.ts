import { useSyncExternalStore } from 'react';
import { publicationLabel } from './hubspotListings';

/** 掲載状況の絞り込み。URL の ?status= が正本。何も無ければ「掲載中」。 */
export type StatusKey = 'live' | 'ended' | 'all' | 'before' | 'unknown';
export const DEFAULT_STATUS: StatusKey = 'live';
const KEYS: readonly StatusKey[] = ['live', 'ended', 'all', 'before', 'unknown'];
/** 一覧の API の publication= に渡す値（すべては空）。 */
export const STATUS_PUBLICATION: Record<StatusKey, string> = { live: '掲載中', ended: '掲載終了', before: '掲載前', unknown: 'unknown', all: '' };
export const STATUS_LABEL: Record<StatusKey, string> = { live: '掲載中', ended: '掲載終了', before: '掲載前', unknown: '状態不明', all: 'すべて' };
export interface StatusCounts { live: number; ended: number; before: number; unknown: number; all: number }

export function parseStatus(search: string): StatusKey {
  const value = new URLSearchParams(search).get('status');
  return KEYS.find(key => key === value) ?? DEFAULT_STATUS;
}
/** 検索部分の ?status= だけを差し替える。既定（掲載中）のときは付けない。 */
export function withStatus(search: string, status: StatusKey): string {
  const params = new URLSearchParams(search);
  if (status === DEFAULT_STATUS) params.delete('status'); else params.set('status', status);
  const text = params.toString();
  return text ? `?${text}` : '';
}
export const statusOf = (publication: string | null | undefined): Exclude<StatusKey, 'all'> => {
  switch (publicationLabel(publication ?? null)) {
    case '掲載中': return 'live';
    case '掲載終了': return 'ended';
    case '掲載前': return 'before';
    default: return 'unknown';
  }
};
export const matchesStatus = (publication: string | null | undefined, status: StatusKey) => status === 'all' || statusOf(publication) === status;
export function countStatuses(rows: readonly { publication_status: string | null }[]): StatusCounts {
  const counts: StatusCounts = { live: 0, ended: 0, before: 0, unknown: 0, all: rows.length };
  for (const row of rows) counts[statusOf(row.publication_status)] += 1;
  return counts;
}

/** サーバが返す公開状況ごとの件数（公開状況以外の絞り込みを反映）から、切り替えに出す件数を作る。 */
export const countsFromServer = (counts: Omit<StatusCounts, 'all'> | undefined): StatusCounts | null => counts ? { ...counts, all: counts.live + counts.ended + counts.before + counts.unknown } : null;

const EVENT = 'jc-status-change';
const subscribe = (notify: () => void) => {
  window.addEventListener(EVENT, notify); window.addEventListener('popstate', notify);
  return () => { window.removeEventListener(EVENT, notify); window.removeEventListener('popstate', notify); };
};
export function setStatus(status: StatusKey) {
  const url = new URL(window.location.href);
  url.search = withStatus(url.search, status);
  window.history.replaceState(window.history.state as unknown, '', url);
  window.dispatchEvent(new Event(EVENT));
}
/** 画面のどこからでも同じ値を読み書きする（URL が正本）。 */
export function useStatusFilter(): [StatusKey, (status: StatusKey) => void] {
  const status = useSyncExternalStore(subscribe, () => parseStatus(window.location.search));
  return [status, setStatus];
}
