import type { CallQueueItem } from '../../generated/CallQueueItem';
import { viewValue } from './propertyModel';
import type { CatalogEntry, CatalogObject } from './propertyModel';

export interface ExtraPart { label: string; text: string }

/**
 * 行に添える項目の値 (詳細条件・項目での並べ替えに使った項目)。表示名が分からない項目は出さない (内部名を見せない)。
 * 値の形は案件の項目の表示と同じ (数値は桁区切り、日付は YYYY/MM/DD、選択肢は表示名、はい/いいえ)。空は「（空）」
 */
export function extraParts(item: CallQueueItem, index: Record<CatalogObject, Map<string, CatalogEntry>> | null): ExtraPart[] {
  if (index === null || item.extra === undefined) return [];
  const out: ExtraPart[] = [];
  for (const e of item.extra) {
    const prop = index.deals.get(e.property)?.prop;
    if (prop === undefined || prop.label.trim() === '') continue;
    const v = viewValue(prop, e.value, new Map());
    out.push({ label: prop.label, text: v.kind === 'empty' ? '（空）' : v.kind === 'links' ? v.urls.join(' ') : v.text.replace(/\s*\n\s*/g, ' ') });
  }
  return out;
}

/** `ラベル 値 · ラベル 値` (title 用の全文) */
export const extraLine = (parts: readonly ExtraPart[]): string => parts.map(p => `${p.label} ${p.text}`).join(' · ');
