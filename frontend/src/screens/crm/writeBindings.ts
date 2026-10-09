// useCrmWrite の状態を、プロパティパネル・案件の概要・衝突の確認が使う形にする。
import { useMemo } from 'react';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { CATALOG_OBJECTS, viewValue } from './propertyModel';
import type { CatalogEntry, CatalogObject } from './propertyModel';
import { WRITE_OBJECT, fieldDefFromCatalog, fieldDefFromEditable, fieldKey, resolveFieldDef, stageKey } from './writeModel';
import type { FieldChange, FieldDef, SaveRequest } from './writeModel';
import type { CrmWrite, FieldStatus, SaveOutcome } from './useCrmWrite';
import type { PipelineChoice, StageRule } from './writeTypes';

type Index = Record<CatalogObject, Map<string, CatalogEntry>>;

/** プロパティパネルの編集 */
export interface PanelWrite {
  /** 編集できるか (null = 編集できる項目の一覧をまだ読めていない / 読めなかった) */
  writesEnabled: boolean | null;
  def(obj: CatalogObject, entry: CatalogEntry): FieldDef | null;
  status(obj: CatalogObject, name: string): FieldStatus | undefined;
  save(def: FieldDef, base: string | null, value: string | null): void;
  retry(req: SaveRequest): void;
}

/** 案件の概要のステージ変更 */
export interface StageMoveCtx {
  writesEnabled: boolean | null;
  pipelineId: string | null;
  stageId: string | null;
  /** いまのパイプラインのステージ */
  stages: { id: string; label: string }[];
  pipelines: PipelineChoice[];
  ruleOf(stageId: string): StageRule | undefined;
  status: FieldStatus | undefined;
  defOf(name: string): FieldDef | null;
  /** 画面に出している案件の、その項目のいまの値 (未取得の項目は HubSpot から読む) */
  currentValues(defs: readonly FieldDef[]): Promise<Record<string, string | null>>;
  move(target: { pipeline_id: string; stage_id: string; label: string }, changes: FieldChange[]): Promise<SaveOutcome>;
  retry(req: SaveRequest): void;
}

export interface ConflictView {
  rows: { name: string; label: string; theirs: string; mine: string }[];
  keepTheirs: () => void;
  keepMine: () => void;
}

export interface WriteBindingsInput {
  write: CrmWrite;
  dealId: string | null;
  data: WorkspaceResponse | null;
  index: Index | null;
  ownerNames: ReadonlyMap<string, string>;
  /** 項目の値を HubSpot から読む (ステージの規則で出す項目のうち、パネルで選んでいないもの) */
  loadValues: (dealId: string, defs: readonly FieldDef[]) => Promise<Record<string, string | null>>;
}

export function useWriteBindings({ write, dealId, data, index, ownerNames, loadValues }: WriteBindingsInput) {
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const primaryContact = data?.contacts.find(c => c.is_primary) ?? data?.contacts[0] ?? null;
  const primaryCompany = data?.companies.find(c => c.is_primary) ?? data?.companies[0] ?? null;
  const fetchedAt = data?.fetched_at ?? null;

  /** 保存した値の仮の表示を重ねた詳細 (読み直した詳細に切り替わったら外れる。反映待ちは残る) */
  const view = useMemo<WorkspaceResponse | null>(() => {
    if (data === null) return null;
    const id = data.deal.id;
    const pick = (obj: CatalogObject, m: Record<string, string | null>) => {
      let out = m;
      for (const name of Object.keys(m)) {
        const o = write.overlay[fieldKey(id, WRITE_OBJECT[obj], name)];
        if (o && (o.pending || o.fetchedAt === data.fetched_at)) out = out === m ? { ...m, [name]: o.value } : { ...out, [name]: o.value };
      }
      return out;
    };
    const so = write.stageOverlay[id];
    const stage = so && (so.pending || so.fetchedAt === data.fetched_at) ? so : null;
    // 古い形の応答 (selected が無い) でも画面を壊さない
    const base = data.selected as WorkspaceResponse['selected'] | undefined;
    if (base === undefined) return stage === null ? data : { ...data, deal: { ...data.deal, stage_id: stage.stage_id, pipeline_id: stage.pipeline_id, stage_label: stage.label } };
    const selected = { deal: pick('deals', base.deal), contact: pick('contacts', base.contact), company: pick('companies', base.company) };
    if (selected.deal === base.deal && selected.contact === base.contact && selected.company === base.company && stage === null) return data;
    return { ...data, selected, deal: stage ? { ...data.deal, stage_id: stage.stage_id, pipeline_id: stage.pipeline_id, stage_label: stage.label } : data.deal };
  }, [data, write.overlay, write.stageOverlay]);

  const recordIds = useMemo(() => ({ contact: primaryContact?.id ?? null, company: primaryCompany?.id ?? null }), [primaryContact, primaryCompany]);

  const panel = useMemo<PanelWrite | null>(() => {
    if (dealId === null) return null;
    return {
      writesEnabled: write.writesEnabled,
      def: (obj, entry) => {
        if (schema === null || write.writesEnabled !== true) return null;
        const o = WRITE_OBJECT[obj];
        if (o === 'contact' && recordIds.contact === null) return null;
        if (o === 'company' && recordIds.company === null) return null;
        return resolveFieldDef(schema, o, entry.prop.name, entry);
      },
      status: (obj, name) => write.statuses[fieldKey(dealId, WRITE_OBJECT[obj], name)],
      save: (def, base, value) => { void write.save({ dealId, changes: [{ object: def.object, name: def.name, base, value }], recordIds, fetchedAt }); },
      retry: req => { void write.save(req); },
    };
  }, [dealId, write, schema, recordIds, fetchedAt]);

  const stage = useMemo<StageMoveCtx | null>(() => {
    if (dealId === null || data === null) return null;
    const defOf = (name: string): FieldDef | null => {
      if (schema === null) return null;
      const listed = schema.editable.find(p => p.name === name && p.object === 'deal') ?? schema.editable.find(p => p.name === name);
      if (listed) return fieldDefFromEditable(listed);
      if (index === null || (schema.read_only ?? []).includes(name)) return null;
      for (const o of CATALOG_OBJECTS) { const e = index[o].get(name); if (e) return fieldDefFromCatalog(WRITE_OBJECT[o], e); }
      return null;
    };
    const pipelineId = data.deal.pipeline_id ?? schema?.pipeline_id ?? null;
    return {
      writesEnabled: write.writesEnabled,
      pipelineId, stageId: data.deal.stage_id,
      stages: schema?.pipelines.find(p => p.id === pipelineId)?.stages ?? [],
      pipelines: schema?.pipelines ?? [],
      ruleOf: id => schema?.stages.find(r => r.id === id),
      status: write.statuses[stageKey(dealId)],
      defOf,
      currentValues: async defs => {
        const out: Record<string, string | null> = {};
        const missing: FieldDef[] = [];
        for (const d of defs) {
          const sel = view?.selected;
          const m = d.object === 'deal' ? sel?.deal : d.object === 'contact' ? sel?.contact : sel?.company;
          if (m && d.name in m) out[d.name] = m[d.name] ?? null; else missing.push(d);
        }
        if (missing.length > 0) Object.assign(out, await loadValues(dealId, missing));
        return out;
      },
      move: (target, changes) => write.save({ dealId, changes, stage: target, recordIds, fetchedAt }),
      retry: req => { void write.save(req); },
    };
  }, [dealId, data, schema, index, write, view, recordIds, fetchedAt, loadValues]);

  const conflict = useMemo<ConflictView | null>(() => {
    const c = write.conflict;
    if (c === null) return null;
    const find = (name: string): { label: string; text: (raw: string | null) => string } => {
      const listed = schema?.editable.find(p => p.name === name);
      let entry: CatalogEntry | undefined;
      if (index !== null) for (const o of CATALOG_OBJECTS) { entry = entry ?? index[o].get(name); }
      const label = listed?.label ?? entry?.prop.label ?? '項目';
      const text = (raw: string | null): string => {
        if (listed && entry === undefined) return raw === null || raw === '' ? '未入力' : (listed.options.find(o => o.value === raw)?.label ?? raw);
        const v = viewValue(entry?.prop, raw, ownerNames);
        return v.kind === 'empty' ? '未入力' : v.kind === 'links' ? v.urls.join(' ') : v.text;
      };
      return { label, text };
    };
    const names = c.changed.length > 0 ? c.changed : Object.keys(c.current);
    return {
      rows: names.map(name => {
        const f = find(name);
        const mine = c.req.changes.find(x => x.name === name);
        return { name, label: f.label, theirs: f.text(c.current[name] ?? null), mine: mine ? f.text(mine.value) : '(このステージへの移動)' };
      }),
      keepTheirs: () => { write.resolveConflict('theirs'); },
      keepMine: () => { write.resolveConflict('mine'); },
    };
  }, [write, schema, index, ownerNames]);

  return { view, panel, stage, conflict };
}
