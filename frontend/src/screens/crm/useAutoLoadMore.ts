import { useEffect, useRef } from 'react';
import type { RefObject } from 'react';

/** 一覧の下端のどれだけ手前で続きを読み始めるか (px) */
export const AUTO_LOAD_MARGIN_PX = 240;

/**
 * 一覧を下端近くまでスクロールしたら続きを 1 回だけ読む。
 *
 * - スクロールする枠 `rootRef` の中の、一覧の下に置いた目印 `sentinelRef` が見えたら `onLoad` を呼ぶ (IntersectionObserver)。
 * - 利用者がスクロールしたときだけ読む (枠の scrollTop が 0 のままなら読まない。開いただけで全ページを読まない)。
 *   目印が初めから見えている短い一覧でも、スクロールした時点で読む (scroll でも確かめる)。
 * - `enabled` の間に呼ぶのは 1 回だけ。読み込み中・失敗中・続きなしは呼び出し側が `enabled=false` にする。
 *   `armKey` (次ページの cursor) が変われば、次のページのためにもう一度だけ呼べるようにする。
 * - IntersectionObserver が無い環境では何もしない (「さらに読み込む」のボタンで読む)。
 */
export function useAutoLoadMore(
  rootRef: RefObject<HTMLElement | null>,
  sentinelRef: RefObject<HTMLElement | null>,
  enabled: boolean,
  armKey: string | null,
  onLoad: () => void,
): void {
  const onLoadRef = useRef(onLoad);
  useEffect(() => { onLoadRef.current = onLoad; });

  useEffect(() => {
    if (!enabled) return;
    const root = rootRef.current;
    const target = sentinelRef.current;
    if (!root || !target || typeof IntersectionObserver === 'undefined') return;
    let fired = false;
    let visible = false;
    const tryLoad = () => {
      if (fired || !visible || root.scrollTop <= 0) return;
      fired = true;
      onLoadRef.current();
    };
    const io = new IntersectionObserver(entries => {
      const last = entries[entries.length - 1];
      if (last) visible = last.isIntersecting;
      tryLoad();
    }, { root, rootMargin: `0px 0px ${String(AUTO_LOAD_MARGIN_PX)}px 0px` });
    io.observe(target);
    root.addEventListener('scroll', tryLoad, { passive: true });
    return () => { io.disconnect(); root.removeEventListener('scroll', tryLoad); };
  }, [enabled, armKey, rootRef, sentinelRef]);
}
