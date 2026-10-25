import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { NavResponse } from '../../generated/NavResponse';

/** 読み直す間隔 (サーバも 60 秒キャッシュ) */
export const FAILED_WRITES_REFRESH_MS = 120_000;
export const FAILED_COUNT_PATH = '/api/admin/crm-operations/failed-count';
export const FAILED_LIST_HREF = '/app/admin?view=crm-failed';

/** 架空サンプルの確認用: `mode=fixture&failedWrites=3` で件数 3 の状態を出す (通信しない) */
function fixtureCount(fixture: boolean): number | null {
  if (!fixture) return null;
  const p = new URLSearchParams(window.location.search);
  // 架空サンプルは通信しない。件数の指定が無ければ 0 (何も出さない)
  const n = Number.parseInt(p.get('failedWrites') ?? '', 10);
  return Number.isFinite(n) && n >= 0 ? n : 0;
}

/**
 * 管理者にだけ、HubSpot に保存できなかった操作の件数を出す。0 件・管理者以外・取れないときは何も出さない。
 * 2 分おきに取り直し、タブが隠れている間は止める。
 */
export function FailedWritesBadge({ fixture = false }: { fixture?: boolean }) {
  const [count, setCount] = useState(() => fixtureCount(fixture) ?? 0);
  useEffect(() => {
    if (fixture) return;
    let stopped = false;
    let isAdmin: boolean | null = null;
    let ctl: AbortController | null = null;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const hidden = () => document.visibilityState === 'hidden';
    const load = async () => {
      ctl?.abort();
      const c = new AbortController();
      ctl = c;
      if (isAdmin === null) {
        const nav = await apiGet<NavResponse>('/api/nav', { signal: c.signal });
        if (stopped || c.signal.aborted) return;
        isAdmin = nav.ok && nav.data.is_admin;
      }
      if (!isAdmin) return;
      const r = await apiGet<{ failed: number }>(FAILED_COUNT_PATH, { signal: c.signal });
      if (stopped || c.signal.aborted) return;
      setCount(r.ok ? r.data.failed : 0);
    };
    const schedule = () => {
      if (timer !== null) clearTimeout(timer);
      timer = null;
      if (stopped || isAdmin === false || hidden()) return;
      timer = setTimeout(() => { if (hidden()) return; void load().catch(() => undefined).finally(schedule); }, FAILED_WRITES_REFRESH_MS);
    };
    const onVisible = () => {
      if (stopped || hidden() || isAdmin === false) return;
      void load().catch(() => undefined).finally(schedule);
    };
    void load().catch(() => undefined).finally(schedule);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      stopped = true;
      ctl?.abort();
      if (timer !== null) clearTimeout(timer);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [fixture]);
  if (count <= 0) return null;
  return (
    <a className="crm-failed-writes" href={FAILED_LIST_HREF} role="status" data-testid="failed-writes-badge">
      HubSpot に保存できなかった操作が {count} 件あります
    </a>
  );
}
