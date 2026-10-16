import { ApiHttpError, apiPatch } from '../../api/client';
import { classifyPatch, liveWriteApi } from '../crm/crmWrite';
import type { WriteApi } from '../crm/crmWrite';
import type { PatchResponse } from '../crm/writeTypes';

/** 受け付け前に断った混雑は、何も保存していないと表示する。CRM 本体の実装は変更しない。 */
export const contractWriteApi: WriteApi = {
  ...liveWriteApi,
  patchDeal: async (id, body) => {
    const result = await apiPatch<PatchResponse>(`/api/crm/deals/${encodeURIComponent(id)}`, body, { timeoutMs: 35_000 });
    if (!result.ok && result.error instanceof ApiHttpError && result.error.status === 503) {
      const body = result.error.body as { error?: string; error_kind?: string } | undefined;
      const error = body?.error ?? body?.error_kind;
      if (error === 'hubspot_busy' || error === 'record_busy') return { kind: 'queue_full' };
    }
    const outcome = classifyPatch(result);
    return outcome.kind === 'error' ? { ...outcome, message: '保存結果を確認できませんでした。' } : outcome;
  },
};
