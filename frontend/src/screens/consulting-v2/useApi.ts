import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';

export type Loaded<T> =
  | { state: 'loading' }
  | { state: 'error'; message: string }
  | { state: 'ready'; data: T };

/** GET して結果を返す。path が null の間は読まない。path が変わると読み直す */
export function useApi<T>(path: string | null, reloadKey = 0): Loaded<T> {
  const [result, setResult] = useState<{ key: string; value: Loaded<T> } | null>(null);
  const key = path === null ? null : `${path}#${String(reloadKey)}`;

  useEffect(() => {
    if (path === null || key === null) return;
    const ctl = new AbortController();
    void apiGet<T>(path, { signal: ctl.signal, timeoutMs: 60_000 }).then((r) => {
      if (ctl.signal.aborted) return;
      setResult({
        key,
        value: r.ok ? { state: 'ready', data: r.data } : { state: 'error', message: r.error.message },
      });
    });
    return () => {
      ctl.abort();
    };
  }, [path, key]);

  if (key === null || result?.key !== key) return { state: 'loading' };
  return result.value;
}
