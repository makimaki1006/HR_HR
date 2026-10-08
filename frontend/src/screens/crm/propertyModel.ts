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
import { HUBSPOT_CARDS, cardPropertyNames } from './hubspotCards';
import type { HubSpotCard } from './hubspotCards';
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
 * 既定の項目: HubSpot の取引レコードの左サイドバーのカード「リスト情報」「BPOアポ情報」の項目 (hubspotCards.ts、カードの並び)。
 * 担当者・会社の項目は既定では出さない (「表示する項目を選ぶ」で足せる)
 */
export const DEFAULT_SELECTED: SelectedProps = {
  deals: cardPropertyNames(),
  contacts: [],
  companies: [],
};

/** 以前 (v1) の既定。v1 に残っていた選択がこれと同じなら、新しい既定 (HubSpot のカード) に移す */
export const LEGACY_DEFAULT_SELECTED: SelectedProps = {
  deals: ['hubspot_owner_id', 'bpo_13', 'bpo_14', 'bpo_20', 'bpo_10', 'bpo_3', 'bpo_4', 'bpo_32'],
  contacts: ['lastname', 'firstname', 'phone'],
  companies: ['website'],
};

export const PROPS_STORAGE_KEY = 'hrhr.crm.selectedProps.v2';
/** 以前の置き場所 (読むだけ。新しい置き場所に移したら消す) */
export const LEGACY_PROPS_STORAGE_KEY = 'hrhr.crm.selectedProps.v1';
const VERSION = 2;
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

function parseVersioned(raw: string | null, version: number): SelectedProps | null {
  if (raw === null) return null;
  let v: unknown;
  try { v = JSON.parse(raw); } catch { return null; }
  if (typeof v !== 'object' || v === null || (v as { v?: unknown }).v !== version) return null;
  const o = v as Record<string, unknown>;
  const deals = cleanList(o.deals);
  const contacts = cleanList(o.contacts);
  const companies = cleanList(o.companies);
  if (deals === null || contacts === null || companies === null) return null;
  return { deals, contacts, companies };
}

/** 残した選択を読む。壊れていたら既定 */
export function parseSelected(raw: string | null): SelectedProps {
  return parseVersioned(raw, VERSION) ?? DEFAULT_SELECTED;
}

/**
 * 以前 (v1) の選択を新しい形にする。v1 の既定のままなら新しい既定 (HubSpot のカード)、
 * 自分で選び直していたらその選択を残す。読めなければ null
 */
export function migrateLegacySelected(raw: string | null): SelectedProps | null {
  const old = parseVersioned(raw, 1);
  if (old === null) return null;
  return sameSelected(old, LEGACY_DEFAULT_SELECTED) ? DEFAULT_SELECTED : old;
}

export function loadSelected(storage: Storage | null): SelectedProps {
  if (storage === null) return DEFAULT_SELECTED;
  try {
    const cur = storage.getItem(PROPS_STORAGE_KEY);
    if (cur !== null) return parseSelected(cur);
    const migrated = migrateLegacySelected(storage.getItem(LEGACY_PROPS_STORAGE_KEY));
    if (migrated === null) return DEFAULT_SELECTED;
    if (saveSelected(storage, migrated)) storage.removeItem(LEGACY_PROPS_STORAGE_KEY);
    return migrated;
  } catch { return DEFAULT_SELECTED; }
}

export function saveSelected(storage: Storage | null, sel: SelectedProps): boolean {
  if (storage === null) return false;
  try { storage.setItem(PROPS_STORAGE_KEY, JSON.stringify({ v: VERSION, ...sel })); return true; } catch { return false; }
}

export function sameSelected(a: SelectedProps, b: SelectedProps): boolean {
  return CATALOG_OBJECTS.every(o => a[o].join(',') === b[o].join(','));
}

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

/** HubSpot のカード 1 枚分の表示 (選んでいて、項目の一覧にある項目だけ。カードの並び) */
export interface CardSection {
  card: HubSpotCard;
  entries: CatalogEntry[];
  /** 選んでいるが項目の一覧に無い (HubSpot で非表示・削除・機微情報) ため出せない項目の数 */
  unavailable: number;
}

/** 案件の項目を HubSpot のカードごとに分ける。どのカードにも無い選んだ項目は `others` (HubSpot の表示順) */
export function cardSections(sel: SelectedProps, index: Record<CatalogObject, Map<string, CatalogEntry>>, cards: readonly HubSpotCard[] = HUBSPOT_CARDS): { sections: CardSection[]; others: CatalogEntry[] } {
  const chosen = new Set(sel.deals);
  const inCards = new Set<string>();
  const sections = cards.map(card => {
    const entries: CatalogEntry[] = [];
    let unavailable = 0;
    for (const it of card.items) {
      inCards.add(it.name);
      if (!chosen.has(it.name)) continue;
      const e = index.deals.get(it.name);
      if (e === undefined) unavailable += 1;
      else entries.push(e);
    }
    return { card, entries, unavailable };
  });
  const others = orderedSelection('deals', sel, index).filter(e => !inCards.has(e.prop.name));
  return { sections, others };
}

/** カードの項目のうち、項目の一覧にあるもの (「HubSpotのカードから選ぶ」でまとめて選ぶ対象) */
export function cardAvailableNames(card: HubSpotCard, index: Record<CatalogObject, Map<string, CatalogEntry>>): string[] {
  const out: string[] = [];
  for (const it of card.items) if (index.deals.has(it.name) && !out.includes(it.name)) out.push(it.name);
  return out;
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
