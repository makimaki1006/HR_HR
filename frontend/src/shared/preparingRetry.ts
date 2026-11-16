import { useEffect, useRef } from 'react';

/** 準備中の取り直しの間隔の上限。 */
export const PREPARING_MAX_MS = 60_000;

/**
 * ms 後に fn を 1 回呼ぶ。タブが隠れている間は呼ばない。
 * 表示に戻ったとき、予定の時刻を過ぎていればすぐ呼び、まだなら残りを待つ。signal が止まれば呼ばない。
 */
export function runWhenDue(signal: AbortSignal, ms: number, fn: () => void): void {
  if (signal.aborted) return;
  const due = Date.now() + ms;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const stop = () => {
    clearTimeout(timer);
    document.removeEventListener('visibilitychange', onVisibility);
    signal.removeEventListener('abort', stop);
  };
  const arm = () => {
    clearTimeout(timer);
    if (document.hidden) return;
    timer = setTimeout(() => { stop(); fn(); }, Math.max(0, due - Date.now()));
  };
  function onVisibility() { arm(); }
  document.addEventListener('visibilitychange', onVisibility);
  signal.addEventListener('abort', stop, { once: true });
  arm();
}

/** 呼ぶたびに次の待ち時間を返す。first から倍々に、max で止まる。 */
export function backoff(first: number, max = PREPARING_MAX_MS): () => number {
  let next = first;
  return () => { const now = next; next = Math.min(next * 2, max); return now; };
}

/**
 * 準備中の応答のあと、もう一度読むきっかけ (再読み込みの番号を上げる等) を呼ぶ。
 * waiting の間、key (応答) が変わるたびに 1 回だけ予約し、間隔は first から倍々 (最大 60 秒)。
 * 準備が終わった (waiting が false) ら間隔を first に戻す。タブが隠れている間は呼ばない。
 */
export function usePreparingRetry(waiting: boolean, first: number, key: unknown, retry: () => void): void {
  const next = useRef(first);
  const retryRef = useRef(retry);
  useEffect(() => { retryRef.current = retry; });
  useEffect(() => {
    if (!waiting) { next.current = first; return; }
    const ctl = new AbortController();
    const ms = next.current;
    next.current = Math.min(ms * 2, PREPARING_MAX_MS);
    runWhenDue(ctl.signal, ms, () => { retryRef.current(); });
    return () => { ctl.abort(); };
  }, [waiting, first, key]);
}
