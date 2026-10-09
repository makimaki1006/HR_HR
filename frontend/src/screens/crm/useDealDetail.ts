import { useCallback, useEffect, useState } from 'react';
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { detailErrorMessage } from './workspaceModel';
import { fixtureDetail } from './workspaceFixture';
import { applyFakeWrites } from './fakeWrite';
import type { QueueMode } from './queueModel';
import { selectedQuery } from './propertyModel';
import type { SelectedProps } from './propertyModel';

/** `fresh` はサーバの短いキャッシュ (60 秒) を使わずに HubSpot から読み直す (「最新にする」・通話が終わった後) */
export interface DetailFetchOptions { fresh?: boolean }

/** `props` は「プロパティ」パネルで選んだ項目 (同じ読み取りで値も受け取る。HubSpot の呼び出し回数は増えない) */
export type DetailFetch = (dealId: string, signal: AbortSignal, props?: SelectedProps, opts?: DetailFetchOptions) => Promise<ApiResult<WorkspaceResponse>>;

/** 案件の詳細の URL (選んだ項目と `fresh=1`) */
export function detailUrl(dealId: string, props?: SelectedProps, opts?: DetailFetchOptions): string {
  const q = new URLSearchParams(props ? selectedQuery(props).replace(/^\?/u, '') : '');
  if (opts?.fresh === true) q.set('fresh', '1');
  const s = q.toString();
  return `/api/crm/workspace/deals/${encodeURIComponent(dealId)}${s === '' ? '' : `?${s}`}`;
}

export const liveDetailFetch: DetailFetch = (dealId, signal, props, opts) =>
  apiGet<WorkspaceResponse>(detailUrl(dealId, props, opts), { signal, timeoutMs: 35_000 });

/** 架空データ (HubSpot には接続しない)。実データの失敗時には使われない */
export const fixtureDetailFetch: DetailFetch = async (dealId, signal, props) => {
  await Promise.resolve();
  if (signal.aborted) return { ok: false, error: new ApiAbortedError('aborted') };
  const data = fixtureDetail(dealId, props);
  if (data === null) return { ok: false, error: new ApiHttpError(404, { error_kind: 'not_found' }) };
  return { ok: true, data: applyFakeWrites(data) };
};

export type DetailPhase = 'idle' | 'loading' | 'ready' | 'error' | 'forbidden';
export interface DetailState {
  /** この状態を作った要求。現在の要求と違えば「読み込み中」として扱う (古い応答を見せない) */
  reqId: string;
  phase: DetailPhase;
  data: WorkspaceResponse | null;
  message: string;
  errorKind: string | null;
  /** 同じ案件を読み直している (「最新にする」等)。読み直しの間も前の内容を表示したまま */
  refreshing: boolean;
  /** 読み直しに失敗した (前の内容を表示したまま、この文言を添える) */
  refreshError: string | null;
}

const blank = (reqId: string, phase: DetailPhase): DetailState => ({ reqId, phase, data: null, message: '', errorKind: null, refreshing: false, refreshError: null });

/** 状態と、それを作った案件・モード・選んだ項目 (読み直しのときに前の内容を出し続けてよいかの判定に使う) */
interface Raw { baseKey: string; s: DetailState }

/**
 * 選んだ案件の詳細の取得。
 * - 案件・モード・再読み込みが変わったら、古い要求を AbortController で中断する
 * - 応答の案件 ID が選んでいる案件と違えば捨てる (別の案件を表示しない)
 * - 失敗時に架空データへ黙って切り替えない
 * - `refresh()` は同じ案件をサーバのキャッシュを使わずに読み直す (`fresh=1`)。読み直しの間は前の内容を出したまま。
 *   読み直しに失敗しても前の内容は消さず `refreshError` を出す (ただし 401/403 は表示をやめる)
 * - `staleIds` にある案件は、次に読むとき `fresh=1` で読む (通話が終わった案件。読めたら `onFreshLoaded` で知らせる)
 */
export function useDealDetail(dealId: string | null, mode: QueueMode, fetcher?: DetailFetch, props?: SelectedProps,
  staleIds?: ReadonlySet<string>, onFreshLoaded?: (dealId: string) => void) {
  const fetchDetail = fetcher ?? (mode === 'fixture' ? fixtureDetailFetch : liveDetailFetch);
  const [raw, setRaw] = useState<Raw>(() => ({ baseKey: '', s: blank('', 'idle') }));
  // 再読み込みの回数と、`fresh` で読むべき要求 (reqId。その 1 回だけ。同じ案件を後で開き直したときは使わない)
  const [reloadToken, setReloadToken] = useState<{ n: number; freshFor: string | null }>({ n: 0, freshFor: null });
  // 選んだ項目を変えたら読み直す (「表示する項目を選ぶ」で適用したときだけ変わる)
  const propsKey = props ? selectedQuery(props) : '';
  const baseKey = `${dealId ?? ''}|${mode}|${propsKey}`;
  const reqId = `${baseKey}|${String(reloadToken.n)}`;
  const state: DetailState = dealId === null ? blank(reqId, 'idle')
    : raw.s.reqId === reqId ? raw.s
      : raw.baseKey === baseKey && raw.s.phase === 'ready' && raw.s.data !== null ? { ...raw.s, reqId, refreshing: true, refreshError: null }
        : blank(reqId, 'loading');
  // この要求を `fresh` で読むか (要求を出す描画の時点の値。通話が終わった案件の一覧が後で変わっても読み直さない)
  const fresh = dealId !== null && (reloadToken.freshFor === reqId || staleIds?.has(dealId) === true);

  useEffect(() => {
    if (dealId === null) return;
    const ctl = new AbortController();
    void fetchDetail(dealId, ctl.signal, props, fresh ? { fresh: true } : undefined).then(res => {
      if (ctl.signal.aborted) return;
      // 「最新にする」は 1 回だけ (同じ案件を後で開き直したときに、また fresh で読まない)
      if (reloadToken.freshFor === reqId) setReloadToken(t => (t.freshFor === reqId ? { ...t, freshFor: null } : t));
      if (res.ok) {
        if (res.data.deal.id !== dealId) {
          setRaw({ baseKey, s: { ...blank(reqId, 'error'), message: '応答の案件が選んだ案件と一致しなかったため、表示を取りやめました。再読み込みしてください。', errorKind: 'deal_mismatch' } });
          return;
        }
        setRaw({ baseKey, s: { ...blank(reqId, 'ready'), data: res.data } });
        if (fresh) onFreshLoaded?.(dealId);
        return;
      }
      if (res.error instanceof ApiAbortedError) return;
      const body = res.error instanceof ApiHttpError && typeof res.error.body === 'object' && res.error.body !== null
        ? (res.error.body as { error_kind?: unknown }) : null;
      const kind = typeof body?.error_kind === 'string' ? body.error_kind : null;
      const status = res.error instanceof ApiHttpError ? res.error.status : null;
      if (res.error instanceof AuthRequiredError || status === 401) {
        setRaw({ baseKey, s: { ...blank(reqId, 'forbidden'), message: 'ログインが必要です。Google Workspace でログインし直してください。', errorKind: kind ?? 'login_required' } });
      } else if (status === 403) {
        setRaw({ baseKey, s: { ...blank(reqId, 'forbidden'), message: detailErrorMessage(kind, status), errorKind: kind } });
      } else {
        const message = detailErrorMessage(kind, status);
        // 読み直しの失敗: 前の内容は残し、読み直せなかったことだけを添える
        setRaw(prev => (prev.baseKey === baseKey && prev.s.phase === 'ready' && prev.s.data !== null
          ? { baseKey, s: { ...prev.s, reqId, refreshing: false, refreshError: message } }
          : { baseKey, s: { ...blank(reqId, 'error'), message, errorKind: kind } }));
      }
    });
    return () => { ctl.abort(); };
  // 案件・モード・再読み込み・選んだ項目は reqId に入っている。取得関数は mode / fetcher で決まる。
  // fresh は reqId が変わった描画の値を使う (通話が終わった案件の一覧が変わっただけでは読み直さない)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId, fetcher]);

  const reload = useCallback(() => { setReloadToken(t => ({ n: t.n + 1, freshFor: null })); }, []);
  /** サーバのキャッシュを使わずに読み直す (前の内容は出したまま) */
  const refresh = useCallback(() => { setReloadToken(t => ({ n: t.n + 1, freshFor: `${baseKey}|${String(t.n + 1)}` })); }, [baseKey]);
  return { state, reload, refresh };
}
