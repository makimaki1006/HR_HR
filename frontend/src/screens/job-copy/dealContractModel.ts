import type { PropValues } from '../crm/writeTypes';
import type { SaveRequest } from '../crm/writeModel';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';

export const CONTRACT_FIELDS = ['contract_plan', 'contract_type', 'contract_start_date', 'contract_expiration_date', 'contract_period', 'amount', 'discount_amount', 'oubo', 'mensetu', 'naite', 'syoudaku'] as const;
export const DISPLAY_FIELDS = [...CONTRACT_FIELDS, 'sales_amount', 'split_sales_amount'] as const;
export const DELIVERY_PIPELINES = ['21596025', '743961896'];

/** 読み取った生の値を base に残す。日付の表示だけ YYYY-MM-DD にする。 */
export function inputValue(prop: CrmCatalogProperty, value: string | null | undefined): string {
  if (!value) return '';
  if (prop.property_type !== 'date') return value;
  if (/^\d{4}-\d{2}-\d{2}/.test(value)) return value.slice(0, 10);
  const date = new Date(Number(value));
  return Number.isFinite(date.getTime()) ? date.toISOString().slice(0, 10) : value;
}
export function displayValue(prop: CrmCatalogProperty, value: string | null | undefined): string {
  if (value == null || value === '') return '未入力';
  return prop.options.find(option => option.value === value)?.label ?? inputValue(prop, value);
}
export function contractRequest(dealId: string, props: CrmCatalogProperty[], base: PropValues, draft: Record<string, string>): SaveRequest {
  return { dealId, changes: props.filter(prop => CONTRACT_FIELDS.some(name => name === prop.name) && prop.name in draft && draft[prop.name] !== inputValue(prop, base[prop.name])).map(prop => ({ object: 'deal', name: prop.name, base: base[prop.name] ?? null, value: draft[prop.name] === '' ? null : draft[prop.name] ?? null })) };
}
