import type { CopyVersion, JobCopyRecord } from './data';
import { jobCopySections } from './JobCopyBody';
import { markInlineChanges } from './diff';
import type { InlineSegment } from './diff';

export const EMPTY_TEXT = '（記載なし）';
export const NOT_RECORDED_TEXT = '記録していません';
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

export interface CompareRow { key: string; label: string; left: string | null; right: string | null; different: boolean }
/** left / right が null の項目は、その版では記録していない（比べられない）。 */
export function compareRows(record: Pick<JobCopyRecord, 'media'>, left: CopyVersion | undefined, right: CopyVersion | undefined): CompareRow[] {
  const a = versionItems(left); const b = versionItems(right);
  const airwork = isAirwork(record);
  const keys = [...new Set([...(airwork ? AIRWORK_RECORDED_ITEMS : []), ...a.keys(), ...b.keys()])];
  return keys.map(key => {
    const x = a.get(key); const y = b.get(key);
    if (airwork && (x === undefined || y === undefined)) return { key, label: itemLabel(key), left: x ?? null, right: y ?? null, different: false };
    return { key, label: itemLabel(key), left: x ?? '', right: y ?? '', different: (x ?? '') !== (y ?? '') };
  });
}
/** 比べられる項目（どちらかの版に記録がある項目）。 */
export const comparableRows = (rows: CompareRow[]) => rows.filter(row => row.left !== null || row.right !== null);

export function markPair(before: string, after: string): [InlineSegment[], InlineSegment[]] {
  const lines = markInlineChanges([{ kind: 'removed', text: before }, { kind: 'added', text: after }]);
  return [lines.find(line => line.kind === 'removed')?.segments ?? [{ text: before, changed: true }], lines.find(line => line.kind === 'added')?.segments ?? [{ text: after, changed: true }]];
}
