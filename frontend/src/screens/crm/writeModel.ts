// 項目の書き換え (PropertyPanel の編集・ステージ変更) の純粋な部分: 入力部品の種類、値の変換、PATCH の本文の組み立て。
import type { CatalogEntry, CatalogObject } from './propertyModel';
import type { EditSchema, EditableProp, EditOption, PatchRequest, PropValues, StageRule, WriteObject } from './writeTypes';

export type FieldKind = 'text' | 'textarea' | 'select' | 'multi' | 'date' | 'number' | 'bool';

export interface FieldDef {
  object: WriteObject;
  /** HubSpot の内部名 (送るときだけ使う。画面には出さない) */
  name: string;
  label: string;
  kind: FieldKind;
  options: EditOption[];
  maxLength?: number | undefined;
}

export const WRITE_OBJECT: Record<CatalogObject, WriteObject> = { deals: 'deal', contacts: 'contact', companies: 'company' };
export const CATALOG_OF: Record<WriteObject, CatalogObject> = { deal: 'deals', contact: 'contacts', company: 'companies' };

interface DefSource { name: string; label: string; type: string; field_type: string; options: readonly { value: string; label: string; hidden?: boolean }[]; max_length?: number | null }

/** HubSpot の種類 → 入力部品。編集できない種類 (日時・選択肢の無い選択式 = 担当者など) は null */
export function kindOf(type: string, fieldType: string, optionCount: number): FieldKind | null {
  if (type === 'bool' || fieldType === 'booleancheckbox') return 'bool';
  if (type === 'enumeration') {
    if (optionCount === 0) return null;
    return fieldType === 'checkbox' ? 'multi' : 'select';
  }
  if (type === 'date') return 'date';
  if (type === 'datetime') return null;
  if (type === 'number') return 'number';
  return fieldType === 'textarea' ? 'textarea' : 'text';
}

/** 項目の定義 → 編集用の定義。システム項目・※編集不可・編集できない種類は null */
export function fieldDefFrom(object: WriteObject, src: DefSource): FieldDef | null {
  if (src.label.includes('※編集不可') || src.name.startsWith('hs_')) return null;
  const options = src.options.filter(o => o.hidden !== true).map(o => ({ value: o.value, label: o.label }));
  const kind = kindOf(src.type, src.field_type, options.length);
  if (kind === null) return null;
  return { object, name: src.name, label: src.label, kind, options, maxLength: src.max_length ?? undefined };
}

export function fieldDefFromEditable(p: EditableProp): FieldDef | null {
  return fieldDefFrom(p.object, p);
}

export function fieldDefFromCatalog(object: WriteObject, e: CatalogEntry): FieldDef | null {
  return fieldDefFrom(object, { name: e.prop.name, label: e.prop.label, type: e.prop.property_type, field_type: e.prop.field_type, options: e.prop.options });
}

/**
 * 画面に出している項目 1 つが書けるか。`editable` の一覧にあれば書ける。
 * `editable_all` のときは、`read_only` に無ければ書ける (定義は表示中の項目の一覧から)。
 */
export function resolveFieldDef(schema: EditSchema, object: WriteObject, name: string, entry: CatalogEntry | undefined): FieldDef | null {
  const listed = schema.editable.find(p => p.object === object && p.name === name);
  if (listed) return fieldDefFromEditable(listed);
  if (schema.editable_all === true && entry !== undefined && !(schema.read_only ?? []).includes(name)) return fieldDefFromCatalog(object, entry);
  return null;
}

/** 編集欄の最初の値 (日付は YYYY-MM-DD、真偽は true/false) */
export function initialDraft(kind: FieldKind, raw: string | null | undefined): string {
  const v = raw ?? '';
  if (kind === 'date') return /^\d{4}-\d{2}-\d{2}/.test(v) ? v.slice(0, 10) : '';
  if (kind === 'bool') return v.toLowerCase() === 'true' ? 'true' : 'false';
  return v;
}

/** 編集欄の値 → HubSpot に送る文字列 (空は null = 消す)。日付は暦日の YYYY-MM-DD (架電結果の bpo_13 と同じ) */
export function draftToValue(kind: FieldKind, draft: string): string | null {
  if (kind === 'bool') return draft === 'true' ? 'true' : 'false';
  const v = kind === 'textarea' ? draft.replace(/\s+$/u, '') : draft.trim();
  return v === '' ? null : v;
}

export const sameValue = (a: string | null | undefined, b: string | null | undefined): boolean => (a ?? '') === (b ?? '');

/** 1 項目の変更 */
export interface FieldChange { object: WriteObject; name: string; base: string | null; value: string | null }

export interface SaveRequest {
  dealId: string;
  changes: FieldChange[];
  stage?: { pipeline_id: string; stage_id: string; label: string } | undefined;
  /** 担当者・会社の項目を書くときの、そのレコードの ID */
  recordIds?: { contact?: string | null; company?: string | null } | undefined;
  /** 編集を始めたときに見ていた詳細を HubSpot から読んだ時刻 (保存した値の仮表示を、読み直した詳細に切り替える目安) */
  fetchedAt?: string | null | undefined;
}

/** 同じ操作の再送 (通信の失敗のあと) かを見分ける鍵。同じ鍵なら同じ operation_id を使う */
export function requestSignature(req: SaveRequest): string {
  return JSON.stringify([req.dealId, req.stage?.pipeline_id ?? null, req.stage?.stage_id ?? null,
    req.changes.map(c => [c.object, c.name, c.base, c.value])]);
}

export function buildPatchBody(req: SaveRequest, operationId: string): PatchRequest {
  const base: PropValues = {};
  const set: PropValues = {};
  const objs: { contact?: { id: string; base: PropValues; set: PropValues }; company?: { id: string; base: PropValues; set: PropValues } } = {};
  for (const c of req.changes) {
    if (c.object === 'deal') { base[c.name] = c.base; set[c.name] = c.value; continue; }
    const id = req.recordIds?.[c.object];
    if (id === undefined || id === null) continue;
    const o = (objs[c.object] ??= { id, base: {}, set: {} });
    o.base[c.name] = c.base;
    o.set[c.name] = c.value;
  }
  const body: PatchRequest = { operation_id: operationId, base, set };
  if (req.stage) body.stage = { pipeline_id: req.stage.pipeline_id, stage_id: req.stage.stage_id };
  if (objs.contact || objs.company) body.objects = objs;
  return body;
}

/** 状態を保存したときの鍵 */
export const fieldKey = (dealId: string, object: WriteObject, name: string): string => `${dealId}|${object}|${name}`;
export const stageKey = (dealId: string): string => `${dealId}|stage`;

/** ステージの規則で出す項目 (shown の順、そのあとに shown に無い required)。 */
export function ruleFieldNames(rule: StageRule | undefined): string[] {
  if (!rule) return [];
  const out = [...rule.shown];
  for (const n of rule.required) if (!out.includes(n)) out.push(n);
  return out;
}

export function missingRequired(rule: StageRule | undefined, values: Readonly<Record<string, string | null | undefined>>): string[] {
  return (rule?.required ?? []).filter(n => (values[n] ?? '').trim() === '');
}
