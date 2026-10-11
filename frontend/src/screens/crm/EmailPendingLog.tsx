import { useEffect, useState } from 'react';
import type { PendingEmailActivity } from '../../generated/PendingEmailActivity';
import { EMAIL_CHANGED, emailMessage, liveEmailApi } from './emailApi';
import { formatTimestamp } from './workspaceModel';

/** HubSpot has no record yet: show only this operator's durable pending/failed sends. */
export function EmailPendingLog({ dealId, sample = false }: { dealId: string; sample?: boolean }) {
  const [rows, setRows] = useState<PendingEmailActivity[]>([]);
  useEffect(() => {
    if (sample || document.getElementById('app-root')?.dataset.crmEmailEnabled !== '1') return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let previousPending = new Set<string>();
    const load = async () => {
      const res = await liveEmailApi.pending(dealId);
      if (!active) return;
      if (res.ok) {
        const current = new Set(res.data.map(r => r.result.operation_id));
        const finished = [...previousPending].some(id => !current.has(id));
        previousPending = new Set(res.data.filter(r => r.result.status === 'pending').map(r => r.result.operation_id));
        setRows(res.data);
        if (finished) window.dispatchEvent(new CustomEvent(EMAIL_CHANGED, { detail: dealId }));
      }
      if (timer) clearTimeout(timer);
      // No repeating audit reads while there is nothing waiting to be recorded.
      if (!res.ok || res.data.some(r => r.result.status === 'pending')) {
        timer = setTimeout(() => { void load(); }, 15_000);
      }
    };
    const changed = () => { void load(); };
    void load();
    window.addEventListener(EMAIL_CHANGED, changed);
    return () => { active = false; if (timer) clearTimeout(timer); window.removeEventListener(EMAIL_CHANGED, changed); };
  }, [dealId, sample]);
  return <ul className="wd-acts">{rows.map(r => <li key={r.result.operation_id} className="wd-act wd-act-email wd-card" data-kind="email">
    <div className="wd-act-head"><span>✉ 自分のメール</span><strong>{r.subject === '' ? 'メール' : r.subject}</strong><small>{formatTimestamp(r.sent_at)}</small></div>
    {r.to && <p>宛先: {r.to}{r.cc.length > 0 && ` · CC: ${r.cc.join(', ')}`}</p>}
    <p className={`crm-email-state crm-email-${r.result.status}`} role="status">{emailMessage(r.result)}</p>
  </li>)}</ul>;
}
