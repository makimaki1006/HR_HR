import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { NavResponse } from '../../generated/NavResponse';

/** ログインしている人のメールアドレスを取る (下書きを、保存した人にだけ見せるために使う) */
export type UserFetch = (signal: AbortSignal) => Promise<ApiResult<{ user_email: string }>>;

export const liveUserFetch: UserFetch = signal => apiGet<NavResponse>('/api/nav', { signal });

export type CurrentUser =
  | { phase: 'loading' }
  | { phase: 'ready'; email: string }
  /** 分からない (未ログイン・通信の失敗)。下書きは画面の中だけで持ち、このタブには残さない */
  | { phase: 'error' };

/** 画面を開いたときに 1 回だけ取る */
export function useCurrentUser(fetcher: UserFetch = liveUserFetch): CurrentUser {
  const [user, setUser] = useState<CurrentUser>({ phase: 'loading' });
  const [fetchFn] = useState(() => fetcher);
  useEffect(() => {
    const ctl = new AbortController();
    void fetchFn(ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      const email = r.ok ? r.data.user_email.trim() : '';
      setUser(email !== '' ? { phase: 'ready', email } : { phase: 'error' });
    }).catch(() => { if (!ctl.signal.aborted) setUser({ phase: 'error' }); });
    return () => { ctl.abort(); };
  }, [fetchFn]);
  return user;
}
