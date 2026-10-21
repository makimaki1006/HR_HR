// 一覧で今見えている行の並び（絞り込み・並べ替え後）。一覧が setVisibleRows で書き、詳細が前後の移動に使う。
// 書かれていないときは、詳細はシートの行の順で移る。
import { useSyncExternalStore } from 'react';

let order: Record<string, number[]> = {};
const listeners = new Set<() => void>();

export function setVisibleRows(sheetId: string, rowNumbers: number[]): void {
  const cur = order[sheetId];
  if (cur?.length === rowNumbers.length && cur.every((n, i) => n === rowNumbers[i])) return;
  order = { ...order, [sheetId]: rowNumbers };
  listeners.forEach((l) => { l(); });
}

export function clearVisibleRows(sheetId: string): void {
  order = Object.fromEntries(Object.entries(order).filter(([k]) => k !== sheetId));
  listeners.forEach((l) => { l(); });
}

export function useVisibleRows(sheetId: string): number[] | undefined {
  return useSyncExternalStore(
    (l) => { listeners.add(l); return () => { listeners.delete(l); }; },
    () => order[sheetId],
  );
}
