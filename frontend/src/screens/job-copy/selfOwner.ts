import { useSyncExternalStore } from 'react';
import { apiGet } from '../../api/client';

/**
 * ログインしている本人の担当者（HubSpot の担当者 ID）。
 * ページを読み込むたびに既存の /api/job-copy/consultant-scope?owner=me で確かめ、このモジュールの中にだけ持つ
 * （ブラウザには保存しない。同じタブで別の人がログインし直しても前の人と取り違えないように）。
 * undefined = まだ分からない、null = 本人の担当者が見つからない（notice 付きの応答・何度か取り直しても読み込めない）、
 * 文字列 = 本人の担当者。どの入口（今日・案件一覧・メニュー）から入っても、同じこの値で「自分」を判断する。
 *
 * サーバは案件の索引を作り直している間（デプロイ直後など数分）preparing を返す。その間は「まだ分からない」のまま
 * 取り直しを続ける（5〜15 秒おき、最初から 15 分まで）。15 分を過ぎたら自動では止めるが、同じページの別の読み込み
 * （今日の画面のホームなど）が揃ったときに recheckSelfOwner() で確かめ直す。
 */
let self: string | null | undefined;
let started = false;
let inFlight = false;
let timer: ReturnType<typeof setTimeout> | null = null;
let firstAt = 0;
let failures = 0;
const listeners = new Set<() => void>();
/** preparing の間、取り直しを続ける長さ。 */
export const PREPARING_LIMIT_MS = 15 * 60_000;
/** failed・通信の失敗を何回まで取り直すか（そのあと「自分なし」）。 */
export const FAILURE_RETRIES = 3;

function publish(value: string | null) { self = value; for (const listener of listeners) listener(); }
/** preparing のときの間隔: 5 秒から始めて 15 秒まで。 */
const preparingDelay = (elapsed: number) => Math.min(15_000, 5_000 + Math.floor(elapsed / 60_000) * 5_000);

function schedule(delay: number) {
  if (timer !== null) clearTimeout(timer);
  timer = setTimeout(() => { timer = null; void load(); }, delay);
}

async function load() {
  if (inFlight || self !== undefined) return;
  inFlight = true;
  let outcome: 'ready' | 'preparing' | 'failed';
  let owner: string | null = null;
  try {
    const result = await apiGet<{ status?: string; owner?: unknown; notice?: unknown }>('/api/job-copy/consultant-scope?owner=me', { timeoutMs: 60_000 });
    const data = result.ok ? result.data as { status?: string; owner?: unknown; notice?: unknown } | null : null;
    if (data?.status === 'ready') {
      outcome = 'ready';
      owner = typeof data.owner === 'string' && data.owner !== '' && data.owner !== 'all' && data.owner !== 'me' && !data.notice ? data.owner : null;
    } else outcome = data?.status === 'preparing' ? 'preparing' : 'failed';
  } catch { outcome = 'failed'; }
  inFlight = false;
  // 待っている間に本人が決まった（テストの setSelfOwnerForTest など）ときは、その値を残す。
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- await の間に変わりうる
  if (self !== undefined) return;
  if (outcome === 'ready') { publish(owner); return; }
  if (outcome === 'preparing') {
    // 索引を作っている間は「まだ分からない」のまま待つ。15 分を過ぎたら自動の取り直しはやめる（recheck は受け付ける）。
    const elapsed = Date.now() - firstAt;
    if (elapsed < PREPARING_LIMIT_MS) schedule(preparingDelay(elapsed));
    return;
  }
  // failed・通信の失敗は、少し間をおいて数回取り直してから「自分なし」にする。
  failures += 1;
  if (failures <= FAILURE_RETRIES) schedule(5_000 * failures);
  else publish(null);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!started) { started = true; firstAt = Date.now(); void load(); }
  return () => { listeners.delete(listener); };
}

/** 本人の担当者。初めて使われたときに読み込み、分かるまで取り直す。 */
export function useSelfOwner(): string | null | undefined {
  return useSyncExternalStore(subscribe, () => self, () => undefined);
}

/**
 * 同じページの別の読み込みが揃ったとき（今日のホームの担当者情報など）に呼ぶ。まだ分からないなら、待たずにすぐ確かめ直す。
 * 分かっている（本人・自分なし）ときや、確かめている最中は何もしない。
 */
export function recheckSelfOwner() {
  if (!started || self !== undefined || inFlight) return;
  if (timer !== null) { clearTimeout(timer); timer = null; }
  void load();
}

/** テスト用: 読み込みをせずに本人を決める（undefined で初めの状態に戻す）。 */
export function setSelfOwnerForTest(value: string | null | undefined) {
  if (timer !== null) { clearTimeout(timer); timer = null; }
  self = value; started = value !== undefined; inFlight = false; failures = 0; firstAt = 0;
  for (const listener of listeners) listener();
}
