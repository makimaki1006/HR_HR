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
    if (outcome.kind === 'invalid' && body.stage) {
      const missing = outcome.body.missing_required;
      // 画面では項目の表示名を MoveModal が補い、ステージ全体の結果も赤にする。
      outcome.body.errors.stage = missing.length > 0 ? '移動に必要な項目が未入力です。入力欄を確認してください。'
        : outcome.body.errors.stage ? '選んだステージへ移せません。同じ取引区分のステージを選び直してください。' : '入力内容を確認してください。';
    }
    return outcome.kind === 'error' ? { ...outcome, message: '保存結果を確認できませんでした。' } : outcome;
  },
};
