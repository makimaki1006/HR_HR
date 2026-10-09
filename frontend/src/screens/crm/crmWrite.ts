import { ApiHttpError, apiGet, apiPatch } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { EditSchema, OperationStatus, PatchConflict, PatchInvalid, PatchRequest, PatchResponse, PatchSaved, PatchQueued } from './writeTypes';

/** PATCH の結果を、画面が分岐しやすい形にしたもの */
export type PatchOutcome =
  | { kind: 'saved'; response: PatchSaved }
  | { kind: 'queued'; response: PatchQueued }
  | { kind: 'conflict'; body: PatchConflict }
  | { kind: 'invalid'; body: PatchInvalid }
  | { kind: 'writes_disabled' }
  | { kind: 'forbidden' }
  | { kind: 'queue_full' }
  /** 通信の失敗・5xx など。HubSpot に届いたか分からない (同じ operation_id で再送してよい) */
  | { kind: 'error'; message: string };

/** 書き込みの窓口。実データは liveWriteApi、架空サンプルは fakeWrite.ts の fakeWriteApi */
export interface WriteApi {
  editSchema(dealId: string, signal: AbortSignal): Promise<ApiResult<EditSchema>>;
  patchDeal(dealId: string, body: PatchRequest): Promise<PatchOutcome>;
  operation(operationId: string): Promise<ApiResult<OperationStatus>>;
}

const obj = (v: unknown): Record<string, unknown> | null => (typeof v === 'object' && v !== null ? v as Record<string, unknown> : null);

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
    if (e.status === 403) return b?.error === 'writes_disabled' ? { kind: 'writes_disabled' } : { kind: 'forbidden' };
    if (e.status === 503 && b?.error === 'queue_full') return { kind: 'queue_full' };
    return { kind: 'error', message: `サーバーがエラーを返しました(HTTP ${String(e.status)})。` };
  }
  return { kind: 'error', message: '通信できませんでした。' };
}

export const liveWriteApi: WriteApi = {
  editSchema: (dealId, signal) => apiGet<EditSchema>(`/api/crm/edit-schema?deal_id=${encodeURIComponent(dealId)}`, { signal, timeoutMs: 35_000 }),
  patchDeal: async (dealId, body) =>
    classifyPatch(await apiPatch<PatchResponse>(`/api/crm/deals/${encodeURIComponent(dealId)}`, body, { timeoutMs: 35_000 })),
  operation: operationId => apiGet<OperationStatus>(`/api/crm/operations/${encodeURIComponent(operationId)}`, { timeoutMs: 15_000 }),
};
