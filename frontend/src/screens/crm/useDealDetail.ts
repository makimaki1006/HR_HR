import { EMAIL_CHANGED, fakeEmailActivities } from "./emailApi";
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { detailErrorMessage } from './workspaceModel';
import { fixtureDetail } from './workspaceFixture';
import { applyFakeWrites } from './fakeWrite';
import type { QueueMode } from './queueModel';
import { selectedQuery } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import { DetailCache, detailCacheFor } from './detailCache';

/** `fresh` はサーバの短いキャッシュ (60 秒) を使わずに HubSpot から読み直す (「最新にする」・通話が終わった後) */
export interface DetailFetchOptions {
  fresh?: boolean;
  /** 先読み (次の行・ポインタを置いた行)。サーバは背景の優先度で読み、混んでいれば 503 を返す。結果は画面のキャッシュに入れるだけ */
  prefetch?: boolean;
}

/** `props` は「プロパティ」パネルで選んだ項目 (同じ読み取りで値も受け取る。HubSpot の呼び出し回数は増えない) */
export type DetailFetch = (dealId: string, signal: AbortSignal, props?: SelectedProps, opts?: DetailFetchOptions) => Promise<ApiResult<WorkspaceResponse>>;

/** 案件の詳細の URL (選んだ項目と `fresh=1`) */
export function detailUrl(dealId: string, props?: SelectedProps, opts?: DetailFetchOptions): string {
  const q = new URLSearchParams(props ? selectedQuery(props).replace(/^\?/u, '') : '');
  if (opts?.fresh === true) q.set('fresh', '1');
  if (opts?.prefetch === true) q.set('prefetch', '1');
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
  const written = applyFakeWrites(data);
  return { ok: true, data: { ...written, activities: [...fakeEmailActivities(dealId), ...written.activities] } };
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
  // この要求を `fresh` で読むか (要求を出す描画の時点の値。通話が終わった案件の一覧が後で変わっても読み直さない)
  const fresh = dealId !== null && (reloadToken.freshFor === reqId || staleIds?.has(dealId) === true);
  // 画面のキャッシュ (60 秒)。`fresh` の読み直しでは使わない。あれば開いた瞬間に出し、裏で読み直して入れ替える
  const cache = detailCacheFor(fetchDetail);
  const cacheKey = DetailCache.key(dealId ?? '', mode, propsKey);
  const cachedData = dealId !== null && !fresh ? cache.get(cacheKey) : null;
  const state: DetailState = dealId === null ? blank(reqId, 'idle')
    : raw.s.reqId === reqId ? raw.s
      : raw.baseKey === baseKey && raw.s.phase === 'ready' && raw.s.data !== null ? { ...raw.s, reqId, refreshing: true, refreshError: null }
        : cachedData !== null ? { ...blank(reqId, 'ready'), data: cachedData }
          : blank(reqId, 'loading');

  useEffect(() => {
    if (dealId === null) return;
    const ctl = new AbortController();
    // 読み直し (fresh) はその案件の古い内容を捨てる。通常の読みは、あれば開いた内容を残したまま裏で確かめる
    if (fresh) cache.dropDeal(dealId);
    const shown = fresh ? null : cache.get(cacheKey);
    void fetchDetail(dealId, ctl.signal, props, fresh ? { fresh: true } : undefined).then(res => {
      if (ctl.signal.aborted) return;
      // 「最新にする」は 1 回だけ (同じ案件を後で開き直したときに、また fresh で読まない)
      if (reloadToken.freshFor === reqId) setReloadToken(t => (t.freshFor === reqId ? { ...t, freshFor: null } : t));
      if (res.ok) {
        if (res.data.deal.id !== dealId) {
          setRaw({ baseKey, s: { ...blank(reqId, 'error'), message: '応答の案件が選んだ案件と一致しなかったため、表示を取りやめました。再読み込みしてください。', errorKind: 'deal_mismatch' } });
          return;
        }
        cache.set(cacheKey, dealId, res.data);
        setRaw({ baseKey, s: { ...blank(reqId, 'ready'), data: res.data } });
        if (fresh) onFreshLoaded?.(dealId);
        return;
      }
      if (res.error instanceof ApiAbortedError) return;
      const body = res.error instanceof ApiHttpError && typeof res.error.body === 'object' && res.error.body !== null
        ? (res.error.body as { error_kind?: unknown }) : null;
      const kind = typeof body?.error_kind === 'string' ? body.error_kind : null;
      const status = res.error instanceof ApiHttpError ? res.error.status : null;
      if (res.error instanceof AuthRequiredError || status === 401 || status === 403) cache.dropDeal(dealId);
      if (res.error instanceof AuthRequiredError || status === 401) {
        setRaw({ baseKey, s: { ...blank(reqId, 'forbidden'), message: 'ログインが必要です。Google Workspace でログインし直してください。', errorKind: kind ?? 'login_required' } });
      } else if (status === 403) {
        setRaw({ baseKey, s: { ...blank(reqId, 'forbidden'), message: detailErrorMessage(kind, status), errorKind: kind } });
      } else {
        const message = detailErrorMessage(kind, status);
        // 読み直しの失敗: 前の内容は残し、読み直せなかったことだけを添える
        setRaw(prev => (prev.baseKey === baseKey && prev.s.phase === 'ready' && prev.s.data !== null
          ? { baseKey, s: { ...prev.s, reqId, refreshing: false, refreshError: message } }
          : shown !== null
            // 画面のキャッシュで開いた内容は、確かめに失敗しても残す (読み直せなかったことを添える)
            ? { baseKey, s: { ...blank(reqId, 'ready'), data: shown, refreshError: message } }
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
  const refresh = useCallback(() => {
    if (dealId !== null) cache.dropDeal(dealId);
    setReloadToken(t => ({ n: t.n + 1, freshFor: `${baseKey}|${String(t.n + 1)}` }));
  }, [baseKey, cache, dealId]);
  useEffect(() => {
    const handler = (e: Event) => { if ((e as CustomEvent<string>).detail === dealId) refresh(); };
    window.addEventListener(EMAIL_CHANGED, handler);
    return () => { window.removeEventListener(EMAIL_CHANGED, handler); };
  }, [dealId, refresh]);
  return { state, reload, refresh };
}

export type PrefetchResult = 'started' | 'cached' | 'busy';

/**
 * 案件の詳細の先読み (次の行・ポインタを置いた行)。結果は画面のキャッシュに入れるだけで、表示は変えない。
 * - 同時に 1 本まで。キャッシュにあれば読まない。失敗 (混雑の 503 を含む) は黙って捨てる
 * - 通話が終わったばかりの案件 (`staleIds`) は読まない (開くとき fresh で読み直すので無駄になる)
 */
export function useDetailPrefetch(mode: QueueMode, fetcher?: DetailFetch, props?: SelectedProps, staleIds?: ReadonlySet<string>) {
  const fetchDetail = fetcher ?? (mode === 'fixture' ? fixtureDetailFetch : liveDetailFetch);
  const propsKey = props ? selectedQuery(props) : '';
  const latest = useRef({ fetchDetail, mode, props, propsKey, staleIds });
  useEffect(() => { latest.current = { fetchDetail, mode, props, propsKey, staleIds }; });
  const inflight = useRef<AbortController | null>(null);
  useEffect(() => () => { inflight.current?.abort(); inflight.current = null; }, []);
  return useCallback((dealId: string): PrefetchResult => {
    const cur = latest.current;
    const cache = detailCacheFor(cur.fetchDetail);
    const key = DetailCache.key(dealId, cur.mode, cur.propsKey);
    if (cache.has(key)) return 'cached';
    if (inflight.current !== null || cur.staleIds?.has(dealId) === true) return 'busy';
    const ctl = new AbortController();
    inflight.current = ctl;
    void cur.fetchDetail(dealId, ctl.signal, cur.props, { prefetch: true }).then(res => {
      if (ctl.signal.aborted) return;
      if (res.ok && res.data.deal.id === dealId) cache.set(key, dealId, res.data);
    }, () => undefined).finally(() => { if (inflight.current === ctl) inflight.current = null; });
    return 'started';
  }, []);
}
