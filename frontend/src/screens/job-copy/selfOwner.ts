import { useSyncExternalStore } from 'react';
import { apiGet } from '../../api/client';

/**
 * ログインしている本人の担当者（HubSpot の担当者 ID）。
 * ページを読み込むたびに 1 回だけ既存の /api/job-copy/consultant-scope?owner=me で確かめ、このモジュールの中にだけ持つ
 * （ブラウザには保存しない。同じタブで別の人がログインし直しても前の人と取り違えないように）。
 * undefined = まだ分からない、null = 本人の担当者が見つからない（notice 付きの応答・読み込めない）、文字列 = 本人の担当者。
 * どの入口（今日・案件一覧・メニュー）から入っても、同じこの値で「自分」を判断する。
 */
let self: string | null | undefined;
let started = false;
const listeners = new Set<() => void>();
const MAX_TRIES = 6;

function publish(value: string | null) { self = value; for (const listener of listeners) listener(); }

async function load(attempt: number) {
  // 読み込めなかったときは「本人の担当者が分からない」にする（画面は担当者名の見出しで動く）。
  try { await loadOnce(attempt); } catch { publish(null); }
}
async function loadOnce(attempt: number) {
  const result = await apiGet<{ status?: string; owner?: unknown; notice?: unknown }>('/api/job-copy/consultant-scope?owner=me', { timeoutMs: 60_000 });
  if (!result.ok) { publish(null); return; }
  const data = result.data as { status?: string; owner?: unknown; notice?: unknown } | null;
  if (data?.status === 'ready') {
    const owner = typeof data.owner === 'string' && data.owner !== '' && data.owner !== 'all' && data.owner !== 'me' && !data.notice ? data.owner : null;
    publish(owner);
    return;
  }
  // 準備中は少し待って取り直す（回数に上限を置き、届かなければ「分からない」のまま）。
  if (data?.status === 'preparing' && attempt + 1 < MAX_TRIES) { window.setTimeout(() => { void load(attempt + 1); }, Math.min(10_000, 2_000 * (attempt + 1))); return; }
  publish(null);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!started) { started = true; void load(0); }
  return () => { listeners.delete(listener); };
}

/** 本人の担当者。初めて使われたときに 1 回だけ読み込む。 */
export function useSelfOwner(): string | null | undefined {
  return useSyncExternalStore(subscribe, () => self, () => undefined);
}

/** テスト用: 読み込みをせずに本人を決める（undefined で初めの状態に戻す）。 */
export function setSelfOwnerForTest(value: string | null | undefined) {
  self = value; started = value !== undefined;
  for (const listener of listeners) listener();
}
