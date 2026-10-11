import { useEffect, useState } from 'react';
import type { ReplySyncStatus as SyncStatus } from '../../generated/ReplySyncStatus';
import { EMAIL_CHANGED, emailChanged, replyStatus, sampleReplyStatus } from './emailApi';

export function checkedMessage(at: string | null, now = Date.now()): string {
  if (at === null) return '返信はまだ確認していません';
  const minutes = Math.max(0, Math.floor((now - Date.parse(at)) / 60_000));
  if (!Number.isFinite(minutes)) return '返信はまだ確認していません';
  return minutes < 1 ? '返信の確認 たった今' : minutes < 60 ? `返信の確認 ${String(minutes)} 分前` : minutes < 1440 ? `返信の確認 ${String(Math.floor(minutes / 60))} 時間前` : `返信の確認 ${String(Math.floor(minutes / 1440))} 日前`;
}
export function ReplySyncStatus({ dealId, sample = false }: { dealId: string; sample?: boolean }) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  useEffect(() => {
    if (!sample && document.getElementById('app-root')?.dataset.crmEmailEnabled !== '1') return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let imported: string | null | undefined;
    const load = async () => {
      const res = await (sample ? sampleReplyStatus() : replyStatus());
      if (!active) return;
      if (res.ok) {
        setStatus(res.data);
        const changed = imported !== undefined && imported !== res.data.last_imported_at;
        imported = res.data.last_imported_at;
        if (changed) emailChanged(dealId);
      }
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { void load(); }, res.ok ? res.data.interval_seconds * 1000 : 600_000);
    };
    const changed = () => { void load(); };
    void load();
    window.addEventListener(EMAIL_CHANGED, changed);
    return () => { active = false; if (timer) clearTimeout(timer); window.removeEventListener(EMAIL_CHANGED, changed); };
  }, [dealId, sample]);
  if (status === null) return null;
  return <p className={status.grant_expired ? 'crm-email-state crm-email-failed' : 'crm-muted'}><small>{status.grant_expired ? <><span>Gmail の許可が切れています</span> · <a href="/api/crm/gmail/authorize">Gmail の許可を更新する</a></> : checkedMessage(status.last_checked_at)}</small></p>;
}
