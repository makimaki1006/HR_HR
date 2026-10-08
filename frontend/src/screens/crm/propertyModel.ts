/**
 * 「プロパティ」パネル: 表示する項目の選択 (このブラウザに残す) と、値の表示の形。
 *
 * - 項目の一覧は GET /api/crm/property-catalog (HubSpot のグループごと。定義だけ)
 * - 選んだ項目の値は、詳細 (GET /api/crm/workspace/deals/{id}) の同じ読み取りで受け取る
 *   (`?deal_props=..&contact_props=..&company_props=..`。HubSpot の呼び出し回数は増えない)
 * - 画面には HubSpot の表示名だけを出し、内部名は出さない
 */
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import type { WorkspaceSelected } from '../../generated/WorkspaceSelected';
import { extractUrls } from './centerLinks';
import { formatPhoneForDisplay } from './phone';
import { formatTimestamp } from './workspaceModel';

export type CatalogObject = 'deals' | 'contacts' | 'companies';
export const CATALOG_OBJECTS: readonly CatalogObject[] = ['deals', 'contacts', 'companies'];
export const OBJECT_LABELS: Record<CatalogObject, string> = { deals: '案件', contacts: '担当者', companies: '会社' };
/** 値を出すレコード (担当者・会社は主のもの) の見出し */
export const OBJECT_VALUE_LABELS: Record<CatalogObject, string> = { deals: '案件', contacts: '担当者(主)', companies: '会社(主)' };

export type SelectedProps = Record<CatalogObject, string[]>;

/** 1 つの型で一度に表示できる項目の最大数 (サーバの MAX_SELECTED_PER_OBJECT と同じ) */
export const MAX_SELECTED_PER_OBJECT = 100;

/**
 * 既定の項目: 以前の「案件」カードにあった架電の項目 (担当・次回架電日/時間・最終架電日・不通時チェック・架電禁止理由・ブロック理由)、
 * URL_求人検索、担当者の名前・電話、会社のサイト
 */
export const DEFAULT_SELECTED: SelectedProps = {
  deals: ['hubspot_owner_id', 'bpo_13', 'bpo_14', 'bpo_20', 'bpo_10', 'bpo_3', 'bpo_4', 'bpo_32'],
  contacts: ['lastname', 'firstname', 'phone'],
  companies: ['website'],
};

export const PROPS_STORAGE_KEY = 'hrhr.crm.selectedProps.v1';
const VERSION = 1;
const NAME_RE = /^[A-Za-z0-9_]{1,100}$/;

function cleanList(v: unknown): string[] | null {
  if (!Array.isArray(v)) return null;
  const out: string[] = [];
  for (const n of v) {
    if (typeof n !== 'string' || !NAME_RE.test(n)) return null;
    if (!out.includes(n)) out.push(n);
  }
  return out.length > MAX_SELECTED_PER_OBJECT ? null : out;
}

/** 残した選択を読む。壊れていたら既定 */
export function parseSelected(raw: string | null): SelectedProps {
  if (raw === null) return DEFAULT_SELECTED;
  let v: unknown;
  try { v = JSON.parse(raw); } catch { return DEFAULT_SELECTED; }
  if (typeof v !== 'object' || v === null || (v as { v?: unknown }).v !== VERSION) return DEFAULT_SELECTED;
  const o = v as Record<string, unknown>;
  const deals = cleanList(o.deals);
  const contacts = cleanList(o.contacts);
  const companies = cleanList(o.companies);
  if (deals === null || contacts === null || companies === null) return DEFAULT_SELECTED;
  return { deals, contacts, companies };
}

export function loadSelected(storage: Storage | null): SelectedProps {
  if (storage === null) return DEFAULT_SELECTED;
  try { return parseSelected(storage.getItem(PROPS_STORAGE_KEY)); } catch { return DEFAULT_SELECTED; }
}

export function saveSelected(storage: Storage | null, sel: SelectedProps): boolean {
  if (storage === null) return false;
  try { storage.setItem(PROPS_STORAGE_KEY, JSON.stringify({ v: VERSION, ...sel })); return true; } catch { return false; }
}

export const sameSelected = (a: SelectedProps, b: SelectedProps) => CATALOG_OBJECTS.every(o => a[o].join(',') === b[o].join(','));

/** 詳細の要求に付けるクエリ (`?deal_props=..`)。何も選んでいなければ '' */
export function selectedQuery(sel: SelectedProps): string {
  const q = new URLSearchParams();
  if (sel.deals.length > 0) q.set('deal_props', sel.deals.join(','));
  if (sel.contacts.length > 0) q.set('contact_props', sel.contacts.join(','));
  if (sel.companies.length > 0) q.set('company_props', sel.companies.join(','));
  const s = q.toString();
  return s === '' ? '' : `?${s}`;
}

export interface CatalogEntry { prop: CrmCatalogProperty; group: string }

/** 型ごとに 内部名 → (定義, グループ名)。並びは HubSpot の表示順 */
export function catalogIndex(catalog: CrmPropertyCatalogResponse): Record<CatalogObject, Map<string, CatalogEntry>> {
  const out: Record<CatalogObject, Map<string, CatalogEntry>> = { deals: new Map(), contacts: new Map(), companies: new Map() };
  for (const o of catalog.objects) {
    const m = (CATALOG_OBJECTS as readonly string[]).includes(o.object_type) ? out[o.object_type as CatalogObject] : null;
    if (m === null) continue;
    for (const g of o.groups) for (const p of g.properties) if (!m.has(p.name)) m.set(p.name, { prop: p, group: g.label });
  }
  return out;
}

/** 一覧に無い項目 (HubSpot で消された・隠された) を選択から外す。変わらなければ同じものを返す */
export function sanitizeSelected(sel: SelectedProps, index: Record<CatalogObject, Map<string, CatalogEntry>>): SelectedProps {
  const next = { deals: sel.deals.filter(n => index.deals.has(n)), contacts: sel.contacts.filter(n => index.contacts.has(n)), companies: sel.companies.filter(n => index.companies.has(n)) };
  return sameSelected(next, sel) ? sel : next;
}

/** 選んだ項目を HubSpot の表示順に並べる (一覧に無いものは除く) */
export function orderedSelection(obj: CatalogObject, sel: SelectedProps, index: Record<CatalogObject, Map<string, CatalogEntry>>): CatalogEntry[] {
  const chosen = new Set(sel[obj]);
  return [...index[obj].values()].filter(e => chosen.has(e.prop.name));
}

/** 選択の切り替え (上限を超えるときは足さない) */
export function toggleSelected(sel: SelectedProps, obj: CatalogObject, names: readonly string[], on: boolean): SelectedProps {
  const cur = sel[obj];
  if (!on) return { ...sel, [obj]: cur.filter(n => !names.includes(n)) };
  const add = names.filter(n => !cur.includes(n));
  if (add.length === 0 || cur.length + add.length > MAX_SELECTED_PER_OBJECT) return sel;
  return { ...sel, [obj]: [...cur, ...add] };
}

export type ValueView =
  | { kind: 'empty' }
  | { kind: 'text'; text: string; multiline?: boolean }
  | { kind: 'links'; urls: string[] };

const BOOL_LABELS: Record<string, string> = { true: 'はい', false: 'いいえ' };

/** 日付 (`YYYY-MM-DD` / ISO / epoch ms) → `YYYY/MM/DD` */
function formatDate(raw: string): string | null {
  const t = raw.trim();
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(t);
  if (m && !/^\d{4}-\d{2}-\d{2}T(?!00:00:00(\.0+)?Z)/.test(t)) return `${m[1] ?? ''}/${m[2] ?? ''}/${m[3] ?? ''}`;
  return formatTimestamp(t)?.slice(0, 10) ?? null;
}

/**
 * 値の表示の形。選択肢は表示名、日付は日本時間、URL はリンク、電話はハイフン区切り、空は「未入力」。
 * 所有者 (hubspot_owner_id 等の所有者の項目) は名前 (分からなければ「担当あり」)
 */
export function viewValue(prop: CrmCatalogProperty | undefined, raw: string | null | undefined, ownerNames: ReadonlyMap<string, string>): ValueView {
  if (raw === null || raw === undefined || raw.trim() === '') return { kind: 'empty' };
  const v = raw.trim();
  if (prop === undefined) return { kind: 'text', text: v };
  const isOwner = prop.name === 'hubspot_owner_id' || (prop.property_type === 'enumeration' && prop.options.length === 0 && /owner/i.test(prop.name));
  if (isOwner) return { kind: 'text', text: ownerNames.get(v) ?? '担当あり' };
  switch (prop.property_type) {
    case 'enumeration': {
      const labels = v.split(';').map(x => x.trim()).filter(Boolean).map(x => prop.options.find(o => o.value === x)?.label ?? x);
      return { kind: 'text', text: labels.join('、') };
    }
    case 'bool': return { kind: 'text', text: BOOL_LABELS[v.toLowerCase()] ?? v };
    case 'date': return { kind: 'text', text: formatDate(v) ?? v };
    case 'datetime': return { kind: 'text', text: formatTimestamp(v) ?? formatDate(v) ?? v };
    case 'number': {
      const n = Number(v);
      return { kind: 'text', text: Number.isFinite(n) && /^-?\d+(\.\d+)?$/.test(v) ? n.toLocaleString('ja-JP') : v };
    }
    default: break;
  }
  if (prop.field_type === 'phonenumber') return { kind: 'text', text: formatPhoneForDisplay(v) ?? v };
  const urls = extractUrls(v);
  // 値が URL だけ (空白・改行・読点区切りで複数でもよい) ならリンクにする
  if (urls.length > 0 && v.split(/[\s,、;]+/u).filter(Boolean).length === urls.length) return { kind: 'links', urls };
  return { kind: 'text', text: v, multiline: v.includes('\n') };
}

/** 選んだ項目の値 (応答の `selected` から) */
export function selectedValue(selected: WorkspaceSelected | undefined, obj: CatalogObject, name: string): string | null | undefined {
  if (selected === undefined) return undefined;
  const m = obj === 'deals' ? selected.deal : obj === 'contacts' ? selected.contact : selected.company;
  return name in m ? m[name] ?? null : undefined;
}
