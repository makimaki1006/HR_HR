import { useSyncExternalStore } from 'react';
import { apiGet } from '../../api/client';

/**
 * ログインしている本人の担当者（HubSpot の担当者 ID）。
 * ページを読み込むたびに既存の /api/job-copy/consultant-scope?owner=me で確かめ、このモジュールの中にだけ持つ
 * （ブラウザには保存しない。同じタブで別の人がログインし直しても前の人と取り違えないように）。
 * undefined = まだ分からない、null = 本人の担当者が見つからない、文字列 = 本人の担当者。
 * どの入口（今日・案件一覧・メニュー）から入っても、同じこの値で「自分」を判断する。
 *
 * - サーバは案件の索引を作り直している間（デプロイ直後など数分）preparing を、作り直しに失敗した直後は failed を返し、
 *   しばらくして自分で作り直す。どちらの間も「まだ分からない」のまま取り直しを続ける（5〜15 秒おき、最初から 15 分まで）。
 * - 通信の失敗・ログイン切れ（401）などは、5・10・15 秒あけて 3 回取り直してから「自分なし」にする（あきらめた null）。
 *   notice 付きの応答（本人の担当者が見つからない）で決めた null とは分け、あきらめた null は recheckSelfOwner() で確かめ直せる。
 * - 使っている画面が無い間・タブが隠れている間は問い合わせない。画面が付いた・表示に戻ったときに、まだ分からなければ確かめ直す。
 * - 同じページの別の読み込み（今日のホームなど）の担当者情報が揃ったときも recheckSelfOwner() で確かめ直す。
 */
let self: string | null | undefined;
/** null があきらめた結果（確かめ直してよい）か。notice で決めた null は false。 */
let gaveUp = false;
let started = false;
let inFlight = false;
/** 問い合わせ中に確かめ直しが来た。終わったら、まだ分からなければすぐもう 1 回。 */
let again = false;
/** 使う画面が無い・タブが隠れているため、取り直しを止めている。 */
let paused = false;
let timer: ReturnType<typeof setTimeout> | null = null;
let firstAt = 0;
let failures = 0;
let watchingVisibility = false;
const listeners = new Set<() => void>();
/** preparing・failed の間、自動の取り直しを続ける長さ。 */
export const PREPARING_LIMIT_MS = 15 * 60_000;
/** 通信の失敗などを何回まで取り直すか（そのあと「自分なし」）。 */
export const FAILURE_RETRIES = 3;
export const SELF_OWNER_TIMEOUT_MS = 60_000;

const decided = () => self !== undefined && !gaveUp;
const active = () => listeners.size > 0 && !document.hidden;
function publish(value: string | null, givenUp = false) { self = value; gaveUp = givenUp; for (const listener of listeners) listener(); }
/** preparing・failed のときの間隔: 最初の 1 分は 5 秒、2 分目は 10 秒、それ以降は 15 秒。 */
export const waitingDelay = (elapsed: number) => Math.min(15_000, 5_000 + Math.floor(elapsed / 60_000) * 5_000);

function clearTimer() { if (timer !== null) { clearTimeout(timer); timer = null; } }
function schedule(delay: number) {
  clearTimer();
  if (!active()) { paused = true; return; }
  timer = setTimeout(() => { timer = null; void load(); }, delay);
}

async function load() {
  if (decided()) return;
  if (inFlight) { again = true; return; }
  if (!active()) { paused = true; return; }
  paused = false; inFlight = true; clearTimer();
  let outcome: 'ready' | 'waiting' | 'error';
  let owner: string | null = null;
  try {
    const result = await apiGet<{ status?: string; owner?: unknown; notice?: unknown }>('/api/job-copy/consultant-scope?owner=me', { timeoutMs: SELF_OWNER_TIMEOUT_MS });
    const data = result.ok ? result.data as { status?: string; owner?: unknown; notice?: unknown } | null : null;
    if (data?.status === 'ready') {
      outcome = 'ready';
      owner = typeof data.owner === 'string' && data.owner !== '' && data.owner !== 'all' && data.owner !== 'me' && !data.notice ? data.owner : null;
    } else outcome = data?.status === 'preparing' || data?.status === 'failed' ? 'waiting' : 'error';
  } catch { outcome = 'error'; }
  inFlight = false;
  const rerun = again; again = false;
  if (decided()) return;
  if (outcome === 'ready') { failures = 0; publish(owner); return; }
  if (outcome === 'waiting') {
    failures = 0;
    // 問い合わせ中に確かめ直しが来ていたら、待たずにもう 1 回。
    if (rerun) { schedule(0); return; }
    // サーバが作り直している間は待つ。15 分を過ぎたら自動の取り直しはやめる（確かめ直しは受け付ける）。
    const elapsed = Date.now() - firstAt;
    if (elapsed < PREPARING_LIMIT_MS) schedule(waitingDelay(elapsed));
    return;
  }
  // 通信の失敗・ログイン切れなどは、少し間をおいて数回取り直してから「自分なし」（あきらめた null）にする。
  failures += 1;
  if (failures <= FAILURE_RETRIES) schedule(5_000 * failures);
  else publish(null, true);
}

function onVisibility() {
  if (document.hidden) { clearTimer(); if (!decided()) paused = true; return; }
  // 表示に戻ったら、まだ分からなければ確かめ直す。
  if (!decided() && listeners.size > 0) recheckSelfOwner();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (!watchingVisibility) { watchingVisibility = true; document.addEventListener('visibilitychange', onVisibility); }
  if (!started) { started = true; firstAt = Date.now(); void load(); }
  else if (paused && !decided()) void load();
  return () => {
    listeners.delete(listener);
    // 使う画面が無くなったら問い合わせを止める（付き直したら続きから）。
    if (listeners.size === 0 && timer !== null) { clearTimer(); paused = true; }
  };
}

/** 本人の担当者。初めて使われたときに読み込み、分かるまで取り直す。 */
export function useSelfOwner(): string | null | undefined {
  return useSyncExternalStore(subscribe, () => self, () => undefined);
}

/**
 * 同じページの別の読み込みが揃ったとき（今日のホームの担当者情報など）や、表示に戻ったときに呼ぶ。
 * まだ分からない・あきらめた null のときは、待たずにすぐ確かめ直す（失敗の回数も戻す）。
 * 問い合わせ中なら、終わったあとにもう 1 回確かめる。notice で決まった null や本人が分かっているときは何もしない。
 */
export function recheckSelfOwner() {
  if (!started || decided()) return;
  failures = 0;
  if (inFlight) { again = true; return; }
  clearTimer();
  void load();
}

/** テスト用: 読み込みをせずに本人を決める（undefined で初めの状態に戻す）。 */
export function setSelfOwnerForTest(value: string | null | undefined) {
  clearTimer();
  self = value; gaveUp = false; started = value !== undefined; inFlight = false; again = false; paused = false; failures = 0; firstAt = 0;
  for (const listener of listeners) listener();
}
