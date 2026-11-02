import { useCallback } from 'react';

/**
 * 開いた一覧の見出しまで画面を動かす。要素に key を付けて、開くたびに作り直されるようにして使う。
 * 一覧の中のページ送りでは作り直されないので、動かない。動きを減らす設定のときはすぐ移る。
 */
export function useRevealOnMount() {
  return useCallback((element: HTMLElement | null) => {
    if (!element || typeof element.scrollIntoView !== 'function') return;
    const reduced = typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    // 描画が済んでから動かす（上のバーに隠れない位置は CSS の scroll-margin-top で決める）。
    requestAnimationFrame(() => { element.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'start' }); });
  }, []);
}
