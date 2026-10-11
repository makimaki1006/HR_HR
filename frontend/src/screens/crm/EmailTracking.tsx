import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { EmailTrackingSummary } from '../../generated/EmailTrackingSummary';
import { formatTimestamp } from './workspaceModel';

const shortDate = (value: string | null): string | null => formatTimestamp(value)?.slice(5) ?? null;

/** Measurements are optional; historic emails and disabled features have no extra label. */
export function EmailTracking({ emailId }: { emailId: string }) {
  const [result, setResult] = useState<{ id: string; data: EmailTrackingSummary } | null>(null);
  useEffect(() => {
    if (!/^\d{1,20}$/.test(emailId)) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      try {
        const response = await apiGet<EmailTrackingSummary>(`/api/crm/emails/${emailId}/tracking`, { signal: controller.signal });
        if (!controller.signal.aborted) {
          if (response.ok) {
            setResult({ id: emailId, data: response.data });
            timer = setTimeout(() => { void load(); }, 60_000);
          } else setResult(null);
        }
      } catch {
        // 204 = no measurements (including CRM_EMAIL_ENABLED off). Never show stale data.
        if (!controller.signal.aborted) setResult(null);
      }
    };
    void load();
    return () => { controller.abort(); if (timer !== undefined) clearTimeout(timer); };
  }, [emailId]);
  const data = result?.id === emailId ? result.data : null;
  if (!data) return null;
  const last = shortDate(data.opens.last_at);
  const first = shortDate(data.opens.first_at);
  const automatic = data.automatic_opens.count + data.automatic_clicks.count > 0;
  return <div className="wd-email-tracking" data-testid="email-tracking">
    <p className="wd-act-meta">開封 {data.opens.count} 回{last && `（最後 ${last}）`}・クリック {data.clicks.count} 回</p>
    <details className="wd-email-tracking-details">
      <summary>開封・クリックの詳細</summary>
      {first && <p>最初の開封 {first}{last && `・最後の開封 ${last}`}</p>}
      {data.links.filter(link => link.clicks.count > 0 || link.automatic_clicks.count > 0).map(link => <p key={link.link_no}>
        <span className="wd-email-tracking-url">{link.url}</span>：クリック {link.clicks.count} 回
        {link.automatic_clicks.count > 0 && `（自動の可能性 ${String(link.automatic_clicks.count)} 回）`}
      </p>)}
      <p className="crm-muted">同じリンクや開封が短時間に繰り返された場合はまとめて数えています。画像の読み込みを数えるため、実際の開封と異なることがあります。</p>
    </details>
    {automatic && <p className="wd-act-meta">自動の可能性：開封 {data.automatic_opens.count} 回・クリック {data.automatic_clicks.count} 回</p>}
  </div>;
}
