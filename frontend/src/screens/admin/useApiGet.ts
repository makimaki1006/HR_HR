// GET-and-render state for one JSON endpoint (W8 admin / my screens).
// Re-fetches when `path` changes; the previous request is aborted on cleanup.
// TODO(platform-team): replace with the shared data hook once it exists.
import { useEffect, useState } from 'react';
import {
  ApiAbortedError,
  ApiError,
  ApiHttpError,
  AuthRequiredError,
  apiGet,
} from '../../api/client';

export type ApiState<T> =
  | { status: 'loading' }
  | { status: 'ok'; data: T }
  | { status: 'error'; error: ApiError };

const LOADING: { status: 'loading' } = { status: 'loading' };

export function useApiGet<T>(path: string): ApiState<T> {
  const [result, setResult] = useState<{ path: string; state: ApiState<T> } | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    void apiGet<T>(path, { signal: controller.signal }).then((r) => {
      if (r.ok) {
        setResult({ path, state: { status: 'ok', data: r.data } });
      } else if (!(r.error instanceof ApiAbortedError)) {
        setResult({ path, state: { status: 'error', error: r.error } });
      }
    });
    return () => {
      controller.abort();
    };
  }, [path]);

  return result !== null && result.path === path ? result.state : LOADING;
}

export interface ApiErrorText {
  title: string;
  detail: string;
}

/**
 * User-facing text for a failed request. 403 is "管理者のみ" because the admin
 * JSON routes sit behind require_admin while /app/admin itself is open to any login.
 */
export function describeApiError(error: ApiError, notFound?: ApiErrorText): ApiErrorText {
  if (error instanceof AuthRequiredError) {
    return { title: 'ログインが必要です', detail: 'ログインし直してから開いてください。' };
  }
  if (error instanceof ApiHttpError) {
    if (error.status === 403) {
      return { title: '管理者のみ', detail: 'この画面は管理者権限のあるアカウントだけが見られます。' };
    }
    if (error.status === 404 && notFound) return notFound;
  }
  return { title: 'サーバ応答を取得できませんでした', detail: error.message };
}
