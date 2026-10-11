import { apiGet, apiPost, ApiHttpError } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { SendEmailRequest } from '../../generated/SendEmailRequest';
import type { EmailResult } from '../../generated/EmailResult';
import type { GmailGrantStatus } from '../../generated/GmailGrantStatus';
import type { PendingEmailActivity } from '../../generated/PendingEmailActivity';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';

export interface EmailApi {
  grant: () => Promise<ApiResult<GmailGrantStatus>>;
  revoke: () => Promise<ApiResult<{ granted: boolean; google_revocation_pending: boolean }>>;
  send: (req: SendEmailRequest) => Promise<ApiResult<EmailResult>>;
  status: (id: string) => Promise<ApiResult<EmailResult>>;
  pending: (dealId: string) => Promise<ApiResult<PendingEmailActivity[]>>;
}
export const liveEmailApi: EmailApi = {
  grant: () => apiGet('/api/crm/gmail/status'),
  revoke: () => apiPost('/api/crm/gmail/revoke', {}),
  send: req => apiPost('/api/crm/emails', req, { timeoutMs: 90_000 }),
  status: id => apiGet(`/api/crm/emails/${encodeURIComponent(id)}`),
  pending: dealId => apiGet(`/api/crm/emails?deal_id=${encodeURIComponent(dealId)}`),
};
export const EMAIL_CHANGED = 'hrhr-email-changed';
export function emailChanged(dealId: string) { window.dispatchEvent(new CustomEvent(EMAIL_CHANGED, { detail: dealId })); }
export function resultFromError(error: unknown): EmailResult | null {
  if (!(error instanceof ApiHttpError) || typeof error.body !== 'object' || error.body === null) return null;
  const body = error.body as Partial<EmailResult>;
  return typeof body.operation_id === 'string' && typeof body.status === 'string' && typeof body.sent === 'boolean' && typeof body.recorded === 'boolean' ? body as EmailResult : null;
}
export function emailMessage(r: EmailResult): string {
  if (r.status === 'saved') return '送信済み・保存済み';
  if (r.status === 'pending') return r.sent ? '送信済み・記録の保存待ち' : '送信結果を確認中です。もう一度送らずにお待ちください。';
  return r.sent ? '送信済み・記録を保存できませんでした。管理者にご連絡ください。' : r.error_kind === 'gmail_send_uncertain' ? '送信結果を確認できませんでした。Gmail の送信済みを確認してください。' : '送信できませんでした。';
}
export function textHtml(text: string): string {
  return `<p>${text.replace(/&/gu, '&amp;').replace(/</gu, '&lt;').replace(/>/gu, '&gt;').replace(/"/gu, '&quot;').replace(/\n/gu, '<br>')}</p>`;
}

// Explicit sample mode only. Never calls Google, HubSpot or the backend send endpoint.
let fakeGranted = false;
const fakeSent = new Map<string, { req: SendEmailRequest; result: EmailResult }>();
export function allowFakeGmail() { fakeGranted = true; }
export const fakeEmailApi: EmailApi = {
  grant: () => Promise.resolve({ ok: true, data: { enabled: true, granted: fakeGranted } }),
  revoke: async () => { await Promise.resolve(); fakeGranted = false; return { ok: true, data: { granted: false, google_revocation_pending: false } }; },
  send: async req => {
    await Promise.resolve();
    if (!fakeGranted) return { ok: false, error: new ApiHttpError(403, { error_kind: 'gmail_grant_required' }) };
    const old = fakeSent.get(req.operation_id);
    if (old) return { ok: true, data: old.result };
    const result = { operation_id: req.operation_id, status: 'saved', sent: true, recorded: true, error_kind: null, hubspot_email_id: `sample-${req.operation_id}` };
    fakeSent.set(req.operation_id, { req, result });
    return { ok: true, data: result };
  },
  status: async id => { await Promise.resolve(); const entry = fakeSent.get(id); return entry ? { ok: true, data: entry.result } : { ok: false, error: new ApiHttpError(404) }; },
  pending: () => Promise.resolve({ ok: true, data: [] }),
};
export function fakeEmailActivities(dealId: string): WorkspaceActivity[] {
  return [...fakeSent.values()].filter(e => e.req.hubspot_deal_id === dealId).map(({ req, result }) => ({
    id: result.hubspot_email_id ?? req.operation_id, kind: 'email', timestamp: new Date().toISOString(), title: req.subject,
    body: null, direction: 'EMAIL', status: 'SENT', duration_ms: null, owner_id: null, source: null, via: 'deal', via_id: dealId,
    rich: { body_html: req.html, body_full: null, from_email: 'taro@example.invalid', from_name: '自分（架空）', to: [req.to], cc: req.cc,
      thread_id: null, attachments_count: 0, start_time: null, end_time: null, location: null, recording_url: null },
  }));
}
