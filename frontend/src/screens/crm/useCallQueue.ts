import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiAbortedError, ApiError, ApiHttpError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueuePartial } from '../../generated/CallQueuePartial';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { fixtureQueuePage } from './queueFixture';
import {
  errorMessage, failureOf, filtersKey, mergeItems, queueApiPath, scopeMatches, unauthorizedMessage, validateFilters,
} from './queueModel';
import type { QueueFilters, QueueMode } from './queueModel';

export type QueueFetch = (
  filters: QueueFilters, cursor: string | null, signal: AbortSignal,
) => Promise<ApiResult<CallQueueResponse>>;

export const liveFetch: QueueFetch = (filters, cursor, signal) =>
  apiGet<CallQueueResponse>(queueApiPath(filters, cursor), { signal, timeoutMs: 35_000 });

/** 架空データ。実データの失敗時には使われない (モードは画面で明示的に選ぶ) */
export const fixtureFetch: QueueFetch = async (filters, cursor, signal) => {
  await Promise.resolve();
  if (signal.aborted) return { ok: false, error: new ApiAbortedError('aborted') };
  return { ok: true, data: fixtureQueuePage(filters, cursor) };
};

export type QueuePhase = 'invalid' | 'loading' | 'ready' | 'error' | 'unauthorized';

export interface QueueState {
  /** この状態を作った要求 (条件・モード・再読み込み)。現在の要求と違えば「読み込み中」として扱う */
  reqId: string;
  phase: QueuePhase;
  items: CallQueueItem[];
  nextCursor: string | null;
  /** 最後に受け取ったページの全体情報 (scope・切り詰め・時刻) */
  last: CallQueueResponse | null;
  /**
   * HubSpot で条件に合う件数 (電話番号なし等を除く前)。先頭ページで分かった値を続きのページでも使う
   * (複数の段階に分かれる並びでは、続きのページの応答は総数を持たない)。分からなければ null
   */
  total: number | null;
  /** 読み込んだページ全体の関連欠落・除外件数 */
  partial: CallQueuePartial | null;
  role: string | null;
  message: string;
  errorKind: string | null;
  invalid: string[];
  loadingMore: boolean;
  moreError: { message: string; kind: string | null } | null;
}

const EMPTY_PARTIAL: CallQueuePartial = {
  missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0,
};

const initial = (reqId = ''): QueueState => ({
  reqId, phase: 'loading', items: [], nextCursor: null, last: null, total: null, partial: null, role: null,
  message: '', errorKind: null, invalid: [], loadingMore: false, moreError: null,
});

function addPartial(a: CallQueuePartial, b: CallQueuePartial): CallQueuePartial {
  return {
    missing_contacts: a.missing_contacts + b.missing_contacts,
    missing_companies: a.missing_companies + b.missing_companies,
    failed: [...new Set([...a.failed, ...b.failed])],
    excluded: {
      no_phone: a.excluded.no_phone + b.excluded.no_phone,
      stop_reason: a.excluded.stop_reason + b.excluded.stop_reason,
      out_of_scope: a.excluded.out_of_scope + b.excluded.out_of_scope,
    },
    // ページごとの件数ではなくパイプラインの設定の数なので、足さずに大きい方
    unknown_stages: Math.max(a.unknown_stages, b.unknown_stages),
  };
}

interface Failure { phase: 'error' | 'unauthorized'; message: string; kind: string | null }

function classify(error: ApiError): Failure {
  const { kind, status } = failureOf(error);
  if (error instanceof AuthRequiredError || (error instanceof ApiHttpError && error.status === 401)) {
    return { phase: 'unauthorized', message: unauthorizedMessage(kind, 401), kind };
  }
  if (error instanceof ApiHttpError && error.status === 403) {
    return { phase: 'unauthorized', message: unauthorizedMessage(kind, 403), kind };
  }
  return { phase: 'error', message: errorMessage(kind, status), kind };
}

export const SCOPE_MISMATCH_MESSAGE = '応答の条件が画面の条件と一致しなかったため、表示を取りやめました。再読み込みしてください。';

/**
 * 架電キューの取得。
 * - 条件 (filters / mode / reload) が変わったら、古い要求を AbortController で中断して先頭から取り直す。
 * - 応答の scope が現在の条件と一致しなければ表示に使わない。
 * - 続きのページ (一覧の下端までのスクロール・「さらに読み込む」) は、直前に表示した次ページの cursor にだけ追記し、deal_id で重複排除する。
 *   読み込み中は次を始めない (同じページを 2 回読まない)。
 * - `refreshKey` が変わったときも取り直す (例: 「次回日が来たものだけ」の間に日付が変わった)。
 */
export function useCallQueue(filters: QueueFilters, mode: QueueMode, fetcher?: QueueFetch, refreshKey = '') {
  const [raw, setState] = useState<QueueState>(() => initial());
  const [reloadToken, setReloadToken] = useState(0);
  const fetchPage = fetcher ?? (mode === 'fixture' ? fixtureFetch : liveFetch);
  const moreCtl = useRef<AbortController | null>(null);
  const moreBusy = useRef(false);
  const filtersRef = useRef(filters);
  // 条件の「中身」が変わったときだけ取り直す (同じ中身の新しいオブジェクトでは取り直さない)
  const key = filtersKey(filters);
  const invalid = validateFilters(filters);
  const hasInvalid = invalid.length > 0;
  const reqId = `${key}|${mode}|${String(reloadToken)}|${refreshKey}`;
  const state: QueueState = raw.reqId === reqId ? raw : initial(reqId);
  const fetchRef = useRef(fetchPage);
  useEffect(() => { filtersRef.current = filters; fetchRef.current = fetchPage; });

  useEffect(() => {
    if (hasInvalid) return; // 条件の誤りは取得せず、返す値 (下) で知らせる
    const ctl = new AbortController();
    moreBusy.current = false;
    void fetchPage(filters, null, ctl.signal).then(res => {
      if (ctl.signal.aborted) return; // 条件が変わった後の古い応答
      if (!res.ok) {
        if (res.error instanceof ApiAbortedError) return;
        const f = classify(res.error);
        setState({ ...initial(reqId), phase: f.phase, message: f.message, errorKind: f.kind });
        return;
      }
      if (!scopeMatches(res.data.scope, filters)) {
        setState({ ...initial(reqId), phase: 'error', message: SCOPE_MISMATCH_MESSAGE, errorKind: 'scope_mismatch' });
        return;
      }
      setState({
        ...initial(reqId), phase: 'ready', items: mergeItems([], res.data.items), nextCursor: res.data.next_cursor,
        last: res.data, total: res.data.total, partial: addPartial(EMPTY_PARTIAL, res.data.partial), role: res.data.scope.role,
      });
    });
    return () => { ctl.abort(); moreCtl.current?.abort(); };
  // 条件の中身・モード・再読み込みは reqId に入っている。fetchPage は mode / fetcher で決まる
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqId, hasInvalid, fetcher]);

  const loadMore = useCallback(() => {
    const cursor = state.nextCursor;
    if (state.phase !== 'ready' || !cursor || moreBusy.current) return;
    const f = filtersRef.current;
    const ctl = new AbortController();
    moreCtl.current = ctl;
    moreBusy.current = true;
    setState(prev => ({ ...prev, loadingMore: true, moreError: null }));
    void fetchRef.current(f, cursor, ctl.signal).then(res => {
      if (ctl.signal.aborted) return;
      moreBusy.current = false;
      if (!res.ok) {
        if (res.error instanceof ApiAbortedError) return;
        const fail = classify(res.error);
        setState(prev => ({ ...prev, loadingMore: false, moreError: { message: fail.message, kind: fail.kind } }));
        return;
      }
      const data = res.data;
      if (!scopeMatches(data.scope, f)) {
        setState(prev => ({ ...prev, loadingMore: false, moreError: { message: SCOPE_MISMATCH_MESSAGE, kind: 'scope_mismatch' } }));
        return;
      }
      setState(prev => {
        // 画面が別のページ (cursor) に進んでいたら追記しない
        if (prev.nextCursor !== cursor) return { ...prev, loadingMore: false };
        return {
          ...prev, loadingMore: false, moreError: null,
          items: mergeItems(prev.items, data.items), nextCursor: data.next_cursor, last: data, total: data.total ?? prev.total,
          partial: addPartial(prev.partial ?? EMPTY_PARTIAL, data.partial), role: data.scope.role,
        };
      });
    });
  }, [state.nextCursor, state.phase]);

  const reload = useCallback(() => { setReloadToken(n => n + 1); }, []);
  // 条件が正しくない間は取得せず、画面に誤りを出す (古い結果は見せない)
  const shown: QueueState = hasInvalid ? { ...initial(reqId), phase: 'invalid', invalid } : state;
  return { state: shown, loadMore, reload };
}
