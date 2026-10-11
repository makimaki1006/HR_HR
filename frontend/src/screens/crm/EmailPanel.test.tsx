// @vitest-environment jsdom
/* eslint-disable @typescript-eslint/require-await -- Immediate Promise fakes match the async API contract. */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { EmailPanel } from './EmailPanel';
import { fixtureDetail } from './workspaceFixture';
import { ApiHttpError, ApiNetworkError } from '../../api/client';
import { emailMessage, textHtml } from './emailApi';
import type { EmailApi } from './emailApi';
import type { EmailResult } from '../../generated/EmailResult';
afterEach(cleanup);
// Fixture ID derives from the existing sample queue, rather than production data.
const detail = () => {
  const d = fixtureDetail('f-1');
  if (!d) throw new Error('sample missing');
  return d;
};
const saved: EmailResult = { operation_id: 'email-op-123456', status: 'saved', sent: true, recorded: true, error_kind: null, hubspot_email_id: '902' };
function api(overrides: Partial<EmailApi> = {}): EmailApi {
  return { grant: async () => ({ ok: true, data: { enabled: true, granted: true } }), revoke: async () => ({ ok: true, data: { granted: false, google_revocation_pending: false } }), send: vi.fn<EmailApi['send']>(async req => ({ ok: true, data: { ...saved, operation_id: req.operation_id } })), status: async () => ({ ok: true, data: saved }), pending: async () => ({ ok: true, data: [] }), ...overrides };
}
async function compose(a: EmailApi) {
  const d = detail();
  render(<EmailPanel data={d} api={a} />);
  fireEvent.click(await screen.findByText('メールを書く'));
  fireEvent.change(screen.getByLabelText('件名'), { target: { value: '採用のご相談' } });
  fireEvent.change(screen.getByLabelText('本文'), { target: { value: '山田様\nご検討ください <資料>' } });
  fireEvent.click(screen.getByRole('button', { name: /^送信$/u }));
  return d;
}
describe('email composer', () => {
  it('hides when the backend gate is closed', async () => {
    const a = api({ grant: async () => ({ ok: true, data: { enabled: false, granted: false } }) });
    const view = render(<EmailPanel data={detail()} api={a} />);
    await waitFor(() => { expect(view.container.querySelector('section')).toBeNull(); });
  });
  it('shows consent link before permission', async () => {
    render(<EmailPanel data={detail()} api={api({ grant: async () => ({ ok: true, data: { enabled: true, granted: false } }) })} />);
    expect((await screen.findByRole('link', { name: 'Gmail で送信を許可する' })).getAttribute('href')).toBe('/api/crm/gmail/authorize');
    expect(screen.getByText('Gmail での送信を許可してください')).toBeTruthy();
    expect(screen.queryByLabelText('件名')).toBeNull();
  });
  it('sends the related contact and escaped concrete body then shows green', async () => {
    const a = api();
    const d = await compose(a);
    await screen.findByText('送信済み・保存済み');
    const send = vi.mocked(a.send);
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0]?.[0]).toMatchObject({ hubspot_deal_id: d.deal.id, contact_id: d.contacts[0]?.id, to: 'sample@example.invalid', cc: [], subject: '採用のご相談', html: '<p>山田様<br>ご検討ください &lt;資料&gt;</p>' });
    expect(send.mock.calls[0]?.[0].operation_id.length).toBe(36);
    expect(screen.getByText('送信済み・保存済み').className).toContain('crm-email-saved');
  });
  it('marks Gmail rejection red without inventing success', async () => {
    await compose(api({ send: async req => ({ ok: false, error: new ApiHttpError(502, { ...saved, operation_id: req.operation_id, status: 'failed', sent: false, recorded: false, error_kind: 'gmail_send_failed' }) }) }));
    expect((await screen.findByText('送信できませんでした。')).className).toContain('crm-email-failed');
  });
  it('marks sent but pending HubSpot record yellow and blocks another send', async () => {
    await compose(api({ send: async req => ({ ok: true, data: { ...saved, operation_id: req.operation_id, status: 'pending', recorded: false } }) }));
    expect((await screen.findByText('送信済み・記録の保存待ち')).className).toContain('crm-email-pending');
    expect(screen.getByRole<HTMLButtonElement>('button', { name: /^送信$/u }).disabled).toBe(true);
  });
  it('checks the original operation after a lost response instead of sending again', async () => {
    const a = api({ send: vi.fn<EmailApi['send']>(async () => ({ ok: false, error: new ApiNetworkError('offline') })), status: vi.fn<EmailApi['status']>(async id => ({ ok: true, data: { ...saved, operation_id: id } })) });
    await compose(a);
    fireEvent.click(await screen.findByText('送信結果を確認'));
    await screen.findByText('送信済み・保存済み');
    expect(a.send).toHaveBeenCalledTimes(1);
    expect(a.status).toHaveBeenCalledWith(vi.mocked(a.send).mock.calls[0]?.[0].operation_id);
  });
  it('403 consent missing opens the permission flow', async () => {
    await compose(api({ send: async () => ({ ok: false, error: new ApiHttpError(403, { error_kind: 'gmail_grant_required' }) }) }));
    expect(await screen.findByRole('link', { name: 'Gmail で送信を許可する' })).toBeTruthy();
  });
  it('removes consent and states remote revocation failure explicitly', async () => {
    render(<EmailPanel data={detail()} api={api({ revoke: async () => ({ ok: true, data: { granted: false, google_revocation_pending: true } }) })} />);
    fireEvent.click(await screen.findByText('Gmail の許可を取り消す'));
    await screen.findByText('この画面からの送信許可を取り消しました。Google アカウントの「接続」からも許可を取り消してください。');
    expect(screen.queryByText('Gmail の許可を取り消す')).toBeNull();
  });
  it('preserves uncertain and sent-failed wording and escapes text', () => {
    expect(emailMessage({ ...saved, status: 'pending', sent: false })).toBe('送信結果を確認中です。もう一度送らずにお待ちください。');
    expect(emailMessage({ ...saved, status: 'failed' })).toBe('送信済み・記録を保存できませんでした。管理者にご連絡ください。');
    expect(textHtml('a&b\n<script>')).toBe('<p>a&amp;b<br>&lt;script&gt;</p>');
  });
});
