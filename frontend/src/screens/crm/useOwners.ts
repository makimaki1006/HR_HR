import { useCallback, useEffect, useState } from 'react';
import { ApiAbortedError, ApiError, ApiHttpError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CrmOwner } from '../../generated/CrmOwner';
import type { CrmOwnersResponse } from '../../generated/CrmOwnersResponse';
import { FIXTURE_OWNERS } from './ownerModel';
import { errorMessage } from './queueModel';

export type OwnersFetch = (signal: AbortSignal) => Promise<ApiResult<CrmOwnersResponse>>;

export const liveOwnersFetch: OwnersFetch = signal =>
  apiGet<CrmOwnersResponse>('/api/crm/owners', { signal, timeoutMs: 30_000 });

export const fixtureOwnersFetch: OwnersFetch = async signal => {
  await Promise.resolve();
  if (signal.aborted) return { ok: false, error: new ApiAbortedError('aborted') };
  return { ok: true, data: { owners: FIXTURE_OWNERS, truncated: false, generated_at: '2026-10-05T03:00:00Z' } };
};

export type OwnersState =
  | { phase: 'idle' }
  | { phase: 'loading' }
  | { phase: 'ready'; owners: CrmOwner[]; truncated: boolean }
  | { phase: 'error'; message: string };

function failureMessage(error: ApiError): string {
  if (error instanceof ApiHttpError) {
    const body = typeof error.body === 'object' && error.body !== null ? (error.body as { error_kind?: unknown }) : null;
    const kind = typeof body?.error_kind === 'string' ? body.error_kind : null;
    if (error.status === 401 || error.status === 403) return '担当者の一覧を見る権限がありません。';
    return errorMessage(kind, error.status);
  }
  return errorMessage(null, null);
}

type Settled = { phase: 'ready'; owners: CrmOwner[]; truncated: boolean } | { phase: 'error'; message: string };

/**
 * 担当者の一覧 (管理者だけ。enabled のときに 1 回取る。絞り込み条件が変わっても取り直さない)。
 * 失敗は黙らず error として返す。画面は担当者 ID の入力に戻せる。reload で取り直せる。
 * 結果は「どの要求のものか」を添えて持ち、いまの要求と違えば読み込み中として扱う (effect 内で state を同期更新しない)。
 */
export function useOwners(enabled: boolean, fetcher: OwnersFetch = liveOwnersFetch) {
  const [nonce, setNonce] = useState(0);
  const [result, setResult] = useState<{ fetcher: OwnersFetch; nonce: number; settled: Settled } | null>(null);

  useEffect(() => {
    if (!enabled) return undefined;
    const ctl = new AbortController();
    const done = (settled: Settled) => {
      if (!ctl.signal.aborted) setResult({ fetcher, nonce, settled });
    };
    void fetcher(ctl.signal).then(r => {
      if (r.ok) done({ phase: 'ready', owners: r.data.owners, truncated: r.data.truncated });
      else done({ phase: 'error', message: failureMessage(r.error) });
    }).catch(() => { done({ phase: 'error', message: errorMessage(null, null) }); });
    return () => { ctl.abort(); };
  }, [enabled, fetcher, nonce]);

  const reload = useCallback(() => { setNonce(n => n + 1); }, []);
  let state: OwnersState = { phase: 'idle' };
  if (enabled) {
    state = result?.fetcher === fetcher && result.nonce === nonce ? result.settled : { phase: 'loading' };
  }
  return { state, reload };
}
