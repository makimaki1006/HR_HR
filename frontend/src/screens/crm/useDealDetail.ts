import { useCallback, useEffect, useState } from 'react';
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { detailErrorMessage } from './workspaceModel';
import { fixtureDetail } from './workspaceFixture';
import type { QueueMode } from './queueModel';

export type DetailFetch = (dealId: string, signal: AbortSignal) => Promise<ApiResult<WorkspaceResponse>>;

export const liveDetailFetch: DetailFetch = (dealId, signal) =>
  apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}`, { signal, timeoutMs: 35_000 });

/** 架空データ (HubSpot には接続しない)。実データの失敗時には使われない */
export const fixtureDetailFetch: DetailFetch = async (dealId, signal) => {
  await Promise.resolve();
  if (signal.aborted) return { ok: false, error: new ApiAbortedError('aborted') };
  const data = fixtureDetail(dealId);
  if (data === null) return { ok: false, error: new ApiHttpError(404, { error_kind: 'not_found' }) };
  return { ok: true, data };
};

export type DetailPhase = 'idle' | 'loading' | 'ready' | 'error' | 'forbidden';
export interface DetailState {
  /** この状態を作った要求。現在の要求と違えば「読み込み中」として扱う (古い応答を見せない) */
  reqId: string;
  phase: DetailPhase;
  data: WorkspaceResponse | null;
  message: string;
  errorKind: string | null;
}

const blank = (reqId: string, phase: DetailPhase): DetailState => ({ reqId, phase, data: null, message: '', errorKind: null });

/**
 * 選んだ案件の詳細の取得。
 * - 案件・モード・再読み込みが変わったら、古い要求を AbortController で中断する
 * - 応答の案件 ID が選んでいる案件と違えば捨てる (別の案件を表示しない)
 * - 失敗時に架空データへ黙って切り替えない
 */
export function useDealDetail(dealId: string | null, mode: QueueMode, fetcher?: DetailFetch) {
  const fetchDetail = fetcher ?? (mode === 'fixture' ? fixtureDetailFetch : liveDetailFetch);
  const [raw, setRaw] = useState<DetailState>(() => blank('', 'idle'));
  const [reloadToken, setReloadToken] = useState(0);
  const reqId = `${dealId ?? ''}|${mode}|${String(reloadToken)}`;
  const state: DetailState = dealId === null ? blank(reqId, 'idle') : raw.reqId === reqId ? raw : blank(reqId, 'loading');

  useEffect(() => {
    if (dealId === null) return;
    const ctl = new AbortController();
    void fetchDetail(dealId, ctl.signal).then(res => {
      if (ctl.signal.aborted) return;
      if (res.ok) {
        if (res.data.deal.id !== dealId) {
          setRaw({ ...blank(reqId, 'error'), message: '応答の案件が選んだ案件と一致しなかったため、表示を取りやめました。再読み込みしてください。', errorKind: 'deal_mismatch' });
          return;
        }
        setRaw({ ...blank(reqId, 'ready'), data: res.data });
        return;
      }
      if (res.error instanceof ApiAbortedError) return;
      const body = res.error instanceof ApiHttpError && typeof res.error.body === 'object' && res.error.body !== null
        ? (res.error.body as { error_kind?: unknown }) : null;
      const kind = typeof body?.error_kind === 'string' ? body.error_kind : null;
      const status = res.error instanceof ApiHttpError ? res.error.status : null;
      if (res.error instanceof AuthRequiredError || status === 401) {
        setRaw({ ...blank(reqId, 'forbidden'), message: 'ログインが必要です。Google Workspace でログインし直してください。', errorKind: kind ?? 'login_required' });
      } else if (status === 403) {
        setRaw({ ...blank(reqId, 'forbidden'), message: detailErrorMessage(kind, status), errorKind: kind });
      } else {
        setRaw({ ...blank(reqId, 'error'), message: detailErrorMessage(kind, status), errorKind: kind });
      }
    });
    return () => { ctl.abort(); };
  // 案件・モード・再読み込みは reqId に入っている。取得関数は mode / fetcher で決まる
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId, fetcher]);

  const reload = useCallback(() => { setReloadToken(n => n + 1); }, []);
  return { state, reload };
}
