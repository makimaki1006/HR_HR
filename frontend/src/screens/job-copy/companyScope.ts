import { useSyncExternalStore } from 'react';

/** 今の取引の画面で、顧客の全求人を出しているとき、左の一覧も同じ範囲にそろえるための共有の値。 */
export interface CompanyScope { dealId: string; dealIds: string[] }
let current: CompanyScope | null = null;
const listeners = new Set<() => void>();
export function setCompanyScope(scope: CompanyScope | null) {
  current = scope;
  listeners.forEach(notify => { notify(); });
}
const subscribe = (notify: () => void) => { listeners.add(notify); return () => { listeners.delete(notify); }; };
export const useCompanyScope = () => useSyncExternalStore(subscribe, () => current);
