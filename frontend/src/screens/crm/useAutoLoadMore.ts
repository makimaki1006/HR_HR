import { useEffect, useRef } from 'react';
import type { RefObject } from 'react';

/** 一覧の下端のどれだけ手前で続きを読み始めるか (px。残りおよそ 1 画面分) */
export const AUTO_LOAD_MARGIN_PX = 600;
/** 利用者がスクロールしないまま、続けて自動で読むページ数の上限 (HubSpot の呼び出しを暴走させない) */
export const AUTO_LOAD_MAX_RUN = 2;

/**
 * 一覧を下端近くまでスクロールしたら続きを自動で読む。
 *
 * - スクロールする枠 `rootRef` の中の、一覧の下に置いた目印 `sentinelRef` が下端の手前 600px に入ったら `onLoad` を呼ぶ (IntersectionObserver)。
 *   目印が最初から見えている短い一覧でも、続きがあれば読む。
 * - 利用者がスクロールしないまま自動で読むのは続けて最大 2 ページまで。スクロールすると数え直して再開する。
 *   `resetKey` (条件の取り直し) が変わっても数え直す。
 * - `enabled` の間に呼ぶのは 1 回だけ。読み込み中・失敗中・続きなしは呼び出し側が `enabled=false` にする。
 *   `armKey` (次ページの cursor) が変われば、次のページのためにもう一度だけ呼べるようにする。
 * - IntersectionObserver が無い環境では何もしない (行をキーボードで最後まで移動しても続きを読む)。
 */
export function useAutoLoadMore(
  rootRef: RefObject<HTMLElement | null>,
  sentinelRef: RefObject<HTMLElement | null>,
  enabled: boolean,
  armKey: string | null,
  resetKey: string,
  onLoad: () => void,
): void {
  const onLoadRef = useRef(onLoad);
  useEffect(() => { onLoadRef.current = onLoad; });
  const run = useRef({ count: 0 });
  // 条件の取り直しで数え直す (下の observer の effect より先に走る)
  useEffect(() => { run.current.count = 0; }, [resetKey]);

  useEffect(() => {
    if (!enabled) return;
    const root = rootRef.current;
    const target = sentinelRef.current;
    if (!root || !target || typeof IntersectionObserver === 'undefined') return;
    let fired = false;
    let visible = false;
    const tryLoad = () => {
      if (fired || !visible || run.current.count >= AUTO_LOAD_MAX_RUN) return;
      fired = true;
      run.current.count += 1;
      onLoadRef.current();
    };
    const io = new IntersectionObserver(entries => {
      const last = entries[entries.length - 1];
      if (last) visible = last.isIntersecting;
      tryLoad();
    }, { root, rootMargin: `0px 0px ${String(AUTO_LOAD_MARGIN_PX)}px 0px` });
    io.observe(target);
    // 利用者のスクロールで回数を数え直し、目印が見えていれば続きを読む
    const onScroll = () => { run.current.count = 0; tryLoad(); };
    root.addEventListener('scroll', onScroll, { passive: true });
    return () => { io.disconnect(); root.removeEventListener('scroll', onScroll); };
  }, [enabled, armKey, rootRef, sentinelRef]);
}
