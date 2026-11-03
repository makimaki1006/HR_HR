// 実画面の時計。固定日時は試作の入口からだけ渡す。
let previewNow: number | null = null;
const dayAt = (milliseconds: number): number => Math.floor((milliseconds + 9 * 3_600_000) / 86_400_000);
export let TODAY_NUM = dayAt(Date.now());

export function portalNow(): number {
  const value = previewNow ?? Date.now();
  TODAY_NUM = dayAt(value);
  return value;
}

export function portalYear(): number {
  return new Date(portalNow() + 9 * 3_600_000).getUTCFullYear();
}

/** 架空サンプルの時刻は本番の入口では設定しない。 */
export function setPortalPreviewClock(iso: string | null): void {
  if (iso === null) { previewNow = null; portalNow(); return; }
  const value = Date.parse(iso);
  if (!Number.isFinite(value)) throw new Error('試作の日時を確認してください');
  previewNow = value;
  portalNow();
}
