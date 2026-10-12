import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';

export interface PropertyCondition { property: string; operator: string; values: string[] }
export const MAX_PROPERTY_CONDITIONS = 6;
export const MAX_CONDITION_GROUPS = 3;
export const LIMIT_MESSAGE = '条件が多すぎます。条件かまとまりを減らしてください。';
export const OPERATORS: Record<string, string> = {
  IN: 'いずれかに一致', NOT_IN: 'いずれにも一致しない', CONTAINS_TOKEN: 'を含む', NOT_CONTAINS_TOKEN: 'を含まない',
  EQ: 'と一致', NEQ: 'と一致しない', GTE: '以上', LTE: '以下', BETWEEN: '範囲',
  TODAY: '今日', BEFORE_TODAY: '今日より前', LAST_N_DAYS: '過去 N 日以内', NEXT_N_DAYS: '今後 N 日以内',
  IS_TRUE: 'はい', IS_FALSE: 'いいえ', HAS_PROPERTY: '空でない', NOT_HAS_PROPERTY: '空',
};
export function operatorsFor(p: CrmCatalogProperty): { value: string; label: string }[] {
  const ops = p.property_type === 'enumeration' ? ['IN', 'NOT_IN', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : p.property_type === 'string' ? ['CONTAINS_TOKEN', 'NOT_CONTAINS_TOKEN', 'EQ', 'NEQ', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : p.property_type === 'number' ? ['EQ', 'NEQ', 'GTE', 'LTE', 'BETWEEN', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : ['date', 'datetime'].includes(p.property_type) ? ['GTE', 'LTE', 'BETWEEN', 'TODAY', 'BEFORE_TODAY', 'LAST_N_DAYS', 'NEXT_N_DAYS', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : p.property_type === 'bool' ? ['IS_TRUE', 'IS_FALSE', 'NOT_HAS_PROPERTY'] : [];
  return ops.map(value => ({ value, label: p.property_type === 'number' && value === 'EQ' ? '='
    : p.property_type === 'number' && value === 'NEQ' ? '≠'
    : ['date', 'datetime'].includes(p.property_type) && value === 'GTE' ? '以降'
    : ['date', 'datetime'].includes(p.property_type) && value === 'LTE' ? '以前' : OPERATORS[value] ?? '' }));
}
function shaped(c: unknown): c is PropertyCondition {
  return typeof c === 'object' && c !== null
    && 'property' in c && typeof c.property === 'string' && c.property.length > 0 && c.property.length <= 200
    && 'operator' in c && typeof c.operator === 'string' && Object.hasOwn(OPERATORS, c.operator)
    && 'values' in c && Array.isArray(c.values) && c.values.length <= 20
    && c.values.every((v: unknown) => typeof v === 'string' && v.length > 0 && Array.from(v).length <= 200 && !/\p{Cc}/u.test(v));
}
export function parseConditions(raw: string | null): PropertyCondition[] {
  if (!raw || raw.length > 12000) return [];
  try { const cs: unknown = JSON.parse(raw); return Array.isArray(cs) && cs.length <= 6 && cs.every(shaped) ? cs : []; }
  catch { return []; }
}
export function parseConditionGroups(raw: string | null): PropertyCondition[][] {
  if (!raw || raw.length > 36000) return [];
  try {
    const gs: unknown = JSON.parse(raw);
    return Array.isArray(gs) && gs.length <= 3 && gs.every((g: unknown) => Array.isArray(g) && g.length > 0 && g.length <= 6 && g.every(shaped)) ? gs as PropertyCondition[][] : [];
  } catch { return []; }
}
export function conditionLabel(c: PropertyCondition, p: CrmCatalogProperty | undefined): string {
  if (!p) return '追加条件(項目の一覧を確認してください)';
  if (p.property_type === 'bool' && c.operator === 'EQ') return `${p.label}: ${c.values[0] === 'true' ? 'はい' : 'いいえ'}`;
  const op = operatorsFor(p).find(o => o.value === c.operator)?.label ?? '条件';
  if (['LAST_N_DAYS', 'NEXT_N_DAYS'].includes(c.operator)) return `${p.label} ${op.replace('N', c.values[0] ?? '')}`;
  return `${p.label} ${op}${c.values.length ? ` ${c.values.map(v => p.property_type === 'number' && Number.isFinite(Number(v)) ? new Intl.NumberFormat('ja-JP', { maximumFractionDigits: 20 }).format(Number(v)) : p.options.find(o => o.value === v)?.label ?? v).join(c.operator === 'BETWEEN' ? ' 〜 ' : '、')}` : ''}`;
}
export type PropertySort = `property:${string}:${'asc' | 'desc'}`;
export function parsePropertySort(s: string): { property: string; direction: 'asc' | 'desc' } | null {
  const m = /^property:([A-Za-z0-9_]{1,200}):(asc|desc)$/.exec(s);
  return m ? { property: m[1] ?? '', direction: m[2] === 'desc' ? 'desc' : 'asc' } : null;
}
export function sortable(p: CrmCatalogProperty): boolean {
  return ['string', 'number', 'date', 'datetime', 'bool', 'enumeration'].includes(p.property_type) && p.field_type !== 'checkbox';
}
export const PROPERTY_SORT_NOTE = '選んだ項目の順に並べます。段階ごとの優先順は使わず、次回日の条件は絞り込みとして適用します。';
function shiftDate(today: string, days: number): string {
  const d = new Date(`${today}T00:00:00Z`); d.setUTCDate(d.getUTCDate() + days); return d.toISOString().slice(0, 10);
}
/** 架空サンプルの条件評価。実データの相対日付はRustで解決する。 */
export function matchesCondition(c: PropertyCondition, value: string | null, today: string, kind = 'string'): boolean {
  const present = value !== null && value !== '';
  const first = c.values[0] ?? '';
  const cmp = (v: string) => Number.isFinite(Number(value)) && Number.isFinite(Number(v)) ? Number(value) - Number(v) : (value ?? '').localeCompare(v);
  const token = () => {
    const pattern = first.toLowerCase().split('*').map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*');
    return present && new RegExp(`(^|[^\\p{L}\\p{N}])${pattern}($|[^\\p{L}\\p{N}])`, 'u').test(value.toLowerCase());
  };
  switch (c.operator) {
    case 'HAS_PROPERTY': return present;
    case 'NOT_HAS_PROPERTY': return !present;
    case 'IN': return present && c.values.includes(value);
    case 'NOT_IN': return present && !c.values.includes(value);
    case 'EQ': return present && (kind === 'number' ? cmp(first) === 0 : value.toLowerCase() === first.toLowerCase());
    case 'NEQ': return present && (kind === 'number' ? cmp(first) !== 0 : value.toLowerCase() !== first.toLowerCase());
    case 'IS_TRUE': return value === 'true';
    case 'IS_FALSE': return value === 'false';
    case 'CONTAINS_TOKEN': return token();
    case 'NOT_CONTAINS_TOKEN': return present && !token();
    case 'GTE': return present && cmp(first) >= 0;
    case 'LTE': return present && cmp(first) <= 0;
    case 'BETWEEN': return present && cmp(first) >= 0 && cmp(c.values[1] ?? '') <= 0;
    case 'TODAY': return present && value.slice(0, 10) === today;
    case 'BEFORE_TODAY': return present && value < today;
    case 'LAST_N_DAYS': return present && value >= shiftDate(today, -Number(first) + 1) && value <= today;
    case 'NEXT_N_DAYS': return present && value >= today && value <= shiftDate(today, Number(first) - 1);
    default: return false;
  }
}
