import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';

export interface PropertyCondition { property: string; operator: string; values: string[] }
export const MAX_PROPERTY_CONDITIONS = 3;
export const OPERATORS: Record<string, string> = {
  IN: 'いずれかに一致', NOT_IN: 'いずれにも一致しない', CONTAINS_TOKEN: 'を含む', EQ: 'と一致',
  GTE: '以上', LTE: '以下', HAS_PROPERTY: '空でない', NOT_HAS_PROPERTY: '空',
};
export function operatorsFor(p: CrmCatalogProperty): { value: string; label: string }[] {
  const ops = p.property_type === 'enumeration' ? ['IN', 'NOT_IN']
    : p.property_type === 'string' ? ['CONTAINS_TOKEN', 'EQ', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : p.property_type === 'number' ? ['EQ', 'GTE', 'LTE', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : ['date', 'datetime'].includes(p.property_type) ? ['GTE', 'LTE', 'NOT_HAS_PROPERTY', 'HAS_PROPERTY']
    : p.property_type === 'bool' ? ['EQ'] : [];
  return ops.map(value => ({ value, label: p.property_type === 'number' && value === 'EQ' ? '='
    : ['date', 'datetime'].includes(p.property_type) && value === 'GTE' ? '以降'
    : ['date', 'datetime'].includes(p.property_type) && value === 'LTE' ? '以前' : OPERATORS[value] ?? '' }));
}
export function parseConditions(raw: string | null): PropertyCondition[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed) || parsed.length > 3) return [];
    if (!parsed.every((c: unknown) => typeof c === 'object' && c !== null
      && 'property' in c && typeof c.property === 'string' && c.property.length > 0 && c.property.length <= 200
      && 'operator' in c && typeof c.operator === 'string' && Object.hasOwn(OPERATORS, c.operator)
      && 'values' in c && Array.isArray(c.values) && c.values.length <= 20
      && c.values.every((v: unknown) => typeof v === 'string' && v.length > 0 && Array.from(v).length <= 200 && !/\p{Cc}/u.test(v)))) return [];
    return parsed as PropertyCondition[];
  } catch { return []; }
}
export function conditionLabel(c: PropertyCondition, p: CrmCatalogProperty | undefined): string {
  if (!p) return '追加条件(項目の一覧を確認してください)';
  if (p.property_type === 'bool') return `${p.label}: ${c.values[0] === 'true' ? 'はい' : 'いいえ'}`;
  const op = operatorsFor(p).find(o => o.value === c.operator)?.label ?? '条件';
  return `${p.label} ${op}${c.values.length ? ` ${c.values.map(v => p.options.find(o => o.value === v)?.label ?? v).join('、')}` : ''}`;
}
/** 架空サンプルにだけ使う条件評価。実際の一覧はサーバーが絞り込む。 */
export function matchesCondition(c: PropertyCondition, value: string | null): boolean {
  switch (c.operator) {
    case 'HAS_PROPERTY': return value !== null && value !== '';
    case 'NOT_HAS_PROPERTY': return value === null || value === '';
    case 'IN': return value !== null && c.values.includes(value);
    case 'NOT_IN': return value !== null && !c.values.includes(value);
    case 'EQ': return value !== null && value.toLowerCase() === c.values[0]?.toLowerCase();
    case 'CONTAINS_TOKEN': {
      const wanted = c.values[0]?.toLowerCase() ?? '';
      const pattern = wanted.split('*').map(t => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*');
      return value !== null && new RegExp(`(^|[^\\p{L}\\p{N}])${pattern}($|[^\\p{L}\\p{N}])`, 'u').test(value.toLowerCase());
    }
    case 'GTE': return value !== null && (Number.isFinite(Number(value)) ? Number(value) >= Number(c.values[0]) : value >= (c.values[0] ?? ''));
    case 'LTE': return value !== null && (Number.isFinite(Number(value)) ? Number(value) <= Number(c.values[0]) : value <= (c.values[0] ?? ''));
    default: return false;
  }
}
