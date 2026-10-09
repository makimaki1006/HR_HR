import { ApiHttpError, apiGet, apiPatch } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { EditSchema, OperationStatus, PatchConflict, PatchInvalid, PatchPartial, PatchRequest, PatchResponse, PatchSaved, PatchQueued } from './writeTypes';

/** PATCH の結果を、画面が分岐しやすい形にしたもの */
export type PatchOutcome =
  | { kind: 'saved'; response: PatchSaved }
  | { kind: 'queued'; response: PatchQueued }
  | { kind: 'conflict'; body: PatchConflict }
  | { kind: 'invalid'; body: PatchInvalid }
  | { kind: 'writes_disabled' }
  /** `partial` = 複数オブジェクトの保存が途中で止まったとき、すでに HubSpot に書けた分 */
  | { kind: 'forbidden'; partial?: PatchPartial | undefined }
  /** 受け付けられず何も保存されていない (混雑・台帳に記録できない)。同じ操作を新しい operation_id で再試行してよい */
  | { kind: 'queue_full' }
  /** 管理者が破棄した操作。同じ operation_id では送れない */
  | { kind: 'discarded' }
  /** 通信の失敗・5xx など。HubSpot に届いたか分からない (同じ operation_id で再送してよい) */
  | { kind: 'error'; message: string; partial?: PatchPartial | undefined };

/** 書き込みの窓口。実データは liveWriteApi、架空サンプルは fakeWrite.ts の fakeWriteApi */
export interface WriteApi {
  editSchema(dealId: string, signal: AbortSignal): Promise<ApiResult<EditSchema>>;
  patchDeal(dealId: string, body: PatchRequest): Promise<PatchOutcome>;
  operation(operationId: string): Promise<ApiResult<OperationStatus>>;
}

const obj = (v: unknown): Record<string, unknown> | null => (typeof v === 'object' && v !== null ? v as Record<string, unknown> : null);

/** エラー本文の `partial` (途中まで書けた分)。形が違えば無いものとして扱う */
function partialOf(b: Record<string, unknown> | null): PatchPartial | undefined {
  const p = obj(b?.partial);
  const values = obj(p?.values);
  if (p === null || values === null) return undefined;
  return { values: values as PatchPartial['values'], objects_values: (obj(p.objects_values) ?? {}) as PatchPartial['objects_values'] };
}

/** HTTP の結果 → PatchOutcome (テストできるよう分けてある) */
export function classifyPatch(res: ApiResult<PatchResponse>): PatchOutcome {
  if (res.ok) {
    const d = res.data;
    if (d.status === 'saved') return { kind: 'saved', response: d };
    return { kind: 'queued', response: d };
  }
  const e = res.error;
  if (e instanceof ApiHttpError) {
    const b = obj(e.body);
    if (e.status === 409 && b?.status === 'conflict') return { kind: 'conflict', body: b as unknown as PatchConflict };
    if (e.status === 422 && b?.status === 'invalid') return { kind: 'invalid', body: b as unknown as PatchInvalid };
    if (e.status === 403) return b?.error === 'writes_disabled' ? { kind: 'writes_disabled' } : { kind: 'forbidden', partial: partialOf(b) };
    if (e.status === 410 && b?.error === 'discarded') return { kind: 'discarded' };
    // queue_unavailable = 台帳 (監査 DB) に記録できず、書かずに断った。queue_full と同じく何も保存されていない。
    // queue_uncertain (送った後で台帳を更新できなかった) はここに入れない: 保存できたか分からない
    if (e.status === 503 && (b?.error === 'queue_full' || b?.error === 'queue_unavailable')) return { kind: 'queue_full' };
    return { kind: 'error', message: `サーバーがエラーを返しました(HTTP ${String(e.status)})。`, partial: partialOf(b) };
  }
  return { kind: 'error', message: '通信できませんでした。' };
}

export const liveWriteApi: WriteApi = {
  editSchema: (dealId, signal) => apiGet<EditSchema>(`/api/crm/edit-schema?deal_id=${encodeURIComponent(dealId)}`, { signal, timeoutMs: 35_000 }),
  patchDeal: async (dealId, body) =>
    classifyPatch(await apiPatch<PatchResponse>(`/api/crm/deals/${encodeURIComponent(dealId)}`, body, { timeoutMs: 35_000 })),
  operation: operationId => apiGet<OperationStatus>(`/api/crm/operations/${encodeURIComponent(operationId)}`, { timeoutMs: 15_000 }),
};
