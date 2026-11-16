import type { CopyVersion, JobCopyRecord } from './data';
import { jobCopySections } from './JobCopyBody';
import { comparisonWhitespace, markInlineChanges } from './diff';
import type { InlineSegment } from './diff';

export const EMPTY_TEXT = '（記載なし）';
export const NOT_RECORDED_TEXT = '記録していません';
export const movedToText = (label: string) => `（『${label}』へ移動）`;
export const movedFromText = (label: string) => `（『${label}』から移動）`;
/** AirWork の履歴に残る項目。ほかの項目は履歴に残らないので「記録していません」と出す。 */
export const AIRWORK_RECORDED_ITEMS = ['案件名', '仕事内容', '勤務時間', '給与', '職種'];
const ITEM_LABELS: Record<string, string> = { 案件名: '求人名' };
export const itemLabel = (key: string) => ITEM_LABELS[key] ?? key;
export const isAirwork = (record: Pick<JobCopyRecord, 'media'>) => /air/i.test(record.media);

export function versionItems(version: CopyVersion | undefined): Map<string, string> {
  const items = new Map<string, string>();
  if (!version?.body.trim()) return items;
  for (const section of version.bodySections ?? jobCopySections(version.body)) {
    const key = section.heading;
    const text = section.text.replace(/\r\n/g, '\n').trim();
    items.set(key, items.has(key) ? `${items.get(key) ?? ''}\n${text}` : text);
  }
  return items;
}

export interface CompareRow {
  key: string; label: string; left: string | null; right: string | null; different: boolean;
  /** 左（前）の文が右では別の欄へ移っただけのとき、その欄の名前（右は「（『X』へ移動）」と出す）。 */
  movedTo?: string;
  /** 右（後）の文が左では別の欄にあったとき、その欄の名前（左は「（『X』から移動）」と出す）。 */
  movedFrom?: string;
}
const squash = (value: string) => value.replace(/\s+/gu, '');
/** text が、other のうち key 以外の欄にそのまま入っていれば、その欄の名前。 */
function movedInto(text: string, key: string, other: Map<string, string>): string | undefined {
  const needle = squash(text);
  if (!needle) return undefined;
  for (const [otherKey, value] of other) if (otherKey !== key && squash(value).includes(needle)) return itemLabel(otherKey);
  return undefined;
}
/** left / right が null の項目は、その版では記録していない（比べられない）。 */
export function compareRows(record: Pick<JobCopyRecord, 'media'>, left: CopyVersion | undefined, right: CopyVersion | undefined): CompareRow[] {
  const a = versionItems(left); const b = versionItems(right);
  const airwork = isAirwork(record);
  const keys = [...new Set([...(airwork ? AIRWORK_RECORDED_ITEMS : []), ...a.keys(), ...b.keys()])];
  return keys.map(key => {
    const x = a.get(key); const y = b.get(key);
    if (airwork && (x === undefined || y === undefined)) return { key, label: itemLabel(key), left: x ?? null, right: y ?? null, different: false };
    // 空白・改行コードだけの違いは「変わった」にしない（上の「本文：前の本文と同じ」と同じ判定）。
    const row: CompareRow = { key, label: itemLabel(key), left: x ?? '', right: y ?? '', different: comparisonWhitespace(x ?? '') !== comparisonWhitespace(y ?? '') };
    // 欄が移っただけ（同じ文が別の見出しの下にある）なら、消えた・増えたと見せず、移った先・元を示す。
    if (x && !y) { const to = movedInto(x, key, b); if (to) row.movedTo = to; }
    if (y && !x) { const from = movedInto(y, key, a); if (from) row.movedFrom = from; }
    return row;
  });
}
/** 比べられる項目（どちらかの版に記録がある項目）。 */
export const comparableRows = (rows: CompareRow[]) => rows.filter(row => row.left !== null || row.right !== null);

export function markPair(before: string, after: string): [InlineSegment[], InlineSegment[]] {
  const lines = markInlineChanges([{ kind: 'removed', text: before }, { kind: 'added', text: after }]);
  return [lines.find(line => line.kind === 'removed')?.segments ?? [{ text: before, changed: true }], lines.find(line => line.kind === 'added')?.segments ?? [{ text: after, changed: true }]];
}
