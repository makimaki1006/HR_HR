// 架空サンプル用の書き込み (メモリの中だけ。HubSpot にも /api/crm/* にも何も送らない)。
// 画面の流れ (編集 → 保存済み、ステージの必須項目 → 移す) を、バックエンド無しで確かめるためのもの。
import type { ApiResult } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { WriteApi, PatchOutcome } from './crmWrite';
import { DEFAULT_PIPELINE_ID, FIXTURE_PIPELINE_ID } from './queuePipelines';
import type { EditSchema, OperationStatus, PatchRequest, PropValues } from './writeTypes';

interface DealWrites { deal: PropValues; contact: PropValues; company: PropValues; stage: { pipeline_id: string; stage_id: string } | null }
const store = new Map<string, DealWrites>();

/** 書き込みの記録を消す (テスト用。ページを読み直しても消える) */
export function resetFakeWrite(): void { store.clear(); }

const slot = (dealId: string): DealWrites => {
  let s = store.get(dealId);
  if (!s) { s = { deal: {}, contact: {}, company: {}, stage: null }; store.set(dealId, s); }
  return s;
};

const STAGES = [
  ['1095387442', '未済'], ['1095387443', '不通'], ['1095387444', '受付ブロック'], ['1095387445', '不在'], ['1095387446', '担当者ブロック'],
  ['1319310149', '日程確保'], ['1095457875', 'アポ日確定'],
] as const;

export const FAKE_SCHEMA = (dealId: string): EditSchema => ({
  deal_id: dealId, pipeline_id: DEFAULT_PIPELINE_ID, stage_id: null,
  // 表示中の項目はほとんど書ける (システム項目だけ read_only)
  editable: [], editable_all: true, read_only: ['bpo_hsurl', 'bpo_32', 'rikulogi_1', 'hubspot_owner_id'],
  stages: [
    { id: '1095457875', label: 'アポ日確定', required: ['aposyutokubi'], shown: ['aposyutokubi', 'risuto_bikou'] },
  ],
  pipelines: [
    { id: DEFAULT_PIPELINE_ID, label: 'bpo_リクロジ', stages: STAGES.map(([id, label]) => ({ id, label })) },
    { id: FIXTURE_PIPELINE_ID, label: '架空パイプライン(確認用)', stages: [{ id: 'fx-new', label: '新規' }, { id: 'fx-follow', label: '追客' }] },
  ],
  writes_enabled: true,
});

function patch(dealId: string, body: PatchRequest): PatchOutcome {
  const s = slot(dealId);
  const schema = FAKE_SCHEMA(dealId);
  // 同じ画面の中で先に保存した値と、見せた値 (base) が食い違えば衝突
  const conflicts: string[] = [];
  const current: PropValues = {};
  const check = (stored: PropValues, base: PropValues) => {
    for (const [k, b] of Object.entries(base)) if (k in stored && (stored[k] ?? '') !== (b ?? '')) { conflicts.push(k); current[k] = stored[k] ?? null; }
  };
  check(s.deal, body.base);
  if (body.objects?.contact) check(s.contact, body.objects.contact.base);
  if (body.objects?.company) check(s.company, body.objects.company.base);
  if (conflicts.length > 0) return { kind: 'conflict', body: { status: 'conflict', current, changed_by_hubspot: conflicts } };
  if (body.stage) {
    const rule = schema.stages.find(r => r.id === body.stage?.stage_id);
    const missing = (rule?.required ?? []).filter(n => (body.set[n] ?? s.deal[n] ?? '') === '');
    if (missing.length > 0) {
      return { kind: 'invalid', body: { status: 'invalid', errors: {}, missing_required: missing } };
    }
  }
  Object.assign(s.deal, body.set);
  if (body.objects?.contact) Object.assign(s.contact, body.objects.contact.set);
  if (body.objects?.company) Object.assign(s.company, body.objects.company.set);
  if (body.stage) s.stage = { pipeline_id: body.stage.pipeline_id, stage_id: body.stage.stage_id };
  return { kind: 'saved', response: { status: 'saved', values: { ...body.set }, fetched_at: new Date().toISOString() } };
}

export const fakeWriteApi: WriteApi = {
  editSchema: async (dealId): Promise<ApiResult<EditSchema>> => { await Promise.resolve(); return { ok: true, data: FAKE_SCHEMA(dealId) }; },
  patchDeal: async (dealId, body) => { await Promise.resolve(); return patch(dealId, body); },
  operation: async (operationId): Promise<ApiResult<OperationStatus>> => {
    await Promise.resolve();
    return { ok: true, data: { operation_id: operationId, status: 'saved', attempts: 1 } };
  },
};

/** 架空の詳細に、メモリに残した書き込みを重ねる (読み直しても編集が戻らないように) */
export function applyFakeWrites(data: WorkspaceResponse): WorkspaceResponse {
  const w = store.get(data.deal.id);
  if (!w) return data;
  const merge = (cur: Record<string, string | null>, over: PropValues) => Object.fromEntries(Object.entries(cur).map(([k, v]) => [k, k in over ? over[k] ?? null : v]));
  const stageLabel = w.stage ? FAKE_SCHEMA(data.deal.id).pipelines.flatMap(p => p.stages).find(x => x.id === w.stage?.stage_id)?.label ?? null : null;
  return {
    ...data,
    deal: w.stage ? { ...data.deal, stage_id: w.stage.stage_id, pipeline_id: w.stage.pipeline_id, stage_label: stageLabel } : data.deal,
    selected: { deal: merge(data.selected.deal, w.deal), contact: merge(data.selected.contact, w.contact), company: merge(data.selected.company, w.company) },
  };
}
