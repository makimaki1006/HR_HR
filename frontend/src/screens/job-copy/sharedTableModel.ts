/** 案件の一覧・顧客の求人の表で共通の、並べ替えとページ送りの計算。 */

/** 空（null・undefined）は昇順・降順のどちらでも最後に置く。文字は日本語順、それ以外は数として比べる。 */
export function sortRows<T>(rows: readonly T[], value: (row: T) => string | number | boolean | null | undefined, descending: boolean, thenBy?: (a: T, b: T) => number, tieKey?: (row: T) => string): T[] {
  return [...rows].sort((a, b) => {
    const left = value(a) ?? null; const right = value(b) ?? null;
    if (left === null && right !== null) return 1;
    if (right === null && left !== null) return -1;
    const delta = typeof left === 'string' && typeof right === 'string' ? left.localeCompare(right, 'ja') : Number(left) - Number(right);
    return (descending ? -delta : delta) || (thenBy?.(a, b) ?? 0) || (tieKey ? tieKey(a).localeCompare(tieKey(b)) : 0);
  });
}

export function pageCount(total: number, size: number): number { return Math.max(1, Math.ceil(total / size)); }

/** pageIndex は 0 始まり。範囲外は端に寄せる。from・to は表示する範囲の 1 始まりの番号（0 件なら 0）。 */
export function paginate<T>(items: readonly T[], pageIndex: number, size: number): { items: T[]; pageIndex: number; pages: number; from: number; to: number } {
  const pages = pageCount(items.length, size);
  const index = Math.min(pages - 1, Math.max(0, pageIndex));
  const slice = items.slice(index * size, (index + 1) * size);
  return { items: slice, pageIndex: index, pages, from: slice.length ? index * size + 1 : 0, to: index * size + slice.length };
}

/** 見出しの並べ替え矢印。選んでいる列だけ向きを出す。 */
export function sortArrow(active: boolean, descending: boolean): string { return active ? descending ? ' ▼' : ' ▲' : ' ▲▼'; }
