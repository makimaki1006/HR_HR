import { useSyncExternalStore } from 'react';

export const detailQueryKeys = ['feature', 'analysis', 'popup', 'popupListing', 'dialog', 'compareLeft', 'compareRight', 'drawer', 'drawerEvent', 'reasonView'] as const;
const changed = 'job-copy-navigation';
const subscribe = (notify: () => void) => {
  window.addEventListener('popstate', notify);
  window.addEventListener(changed, notify);
  return () => { window.removeEventListener('popstate', notify); window.removeEventListener(changed, notify); };
};
export const useJobCopySearch = () => useSyncExternalStore(subscribe, () => window.location.search);

/** この画面の履歴だけを管理する。既存の query と他の history.state は保持する。 */
export function navigateJobCopy(url: URL, mode: 'push' | 'replace' = 'push') {
  if (url.href === window.location.href) return;
  const state = { ...window.history.state as Record<string, unknown> | null };
  if (mode === 'push') {
    const main = document.querySelector('.jc-main');
    window.history.replaceState({ ...state, jobCopyScroll: main?.scrollTop ?? 0 }, '');
    window.history.pushState({ ...state, jobCopyScroll: 0 }, '', url);
  } else window.history.replaceState(state, '', url);
  window.dispatchEvent(new Event(changed));
}
export function updateJobCopyQuery(values: Record<string, string | null>, mode: 'push' | 'replace' = 'push') {
  const url = new URL(window.location.href);
  for (const [key, value] of Object.entries(values)) {
    if (value === null) url.searchParams.delete(key); else url.searchParams.set(key, value);
  }
  navigateJobCopy(url, mode);
}

/** 顧客報告のスライドの画面の URL（今の画面の指定に ?report=deal:ID / listing:ID を足す）。 */
export function reportUrl(target: { scope: 'deal' | 'listing'; id: string }): string {
  const url = new URL(window.location.href);
  url.searchParams.set('report', `${target.scope}:${target.id}`);
  return url.pathname + url.search;
}
/** 顧客報告のスライドの画面を新しいタブで開く（元の画面の入力途中を消さない）。 */
export function openReport(target: { scope: 'deal' | 'listing'; id: string }) {
  window.open(reportUrl(target), '_blank', 'noopener');
}
export function closeReport() {
  updateJobCopyQuery({ report: null });
}
