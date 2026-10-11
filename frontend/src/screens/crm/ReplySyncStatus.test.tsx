// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { ReplySyncStatus, checkedMessage } from './ReplySyncStatus';
import { EMAIL_CHANGED, replyStatus } from './emailApi';
vi.mock('./emailApi', async importOriginal => ({ ...await importOriginal<typeof import('./emailApi')>(), replyStatus: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); document.body.innerHTML = ''; });
function setup() {
  const root = document.createElement('div'); root.id = 'app-root'; root.dataset.crmEmailEnabled = '1'; document.body.append(root);
}
describe('reply check status', () => {
  it('formats concrete times without implementation terms', () => {
    const now = Date.parse('2026-10-11T12:00:00Z');
    expect(checkedMessage('2026-10-11T11:50:00Z', now)).toBe('返信の確認 10 分前');
    expect(checkedMessage('2026-10-11T11:59:40Z', now)).toBe('返信の確認 たった今');
    expect(checkedMessage('2026-10-11T10:00:00Z', now)).toBe('返信の確認 2 時間前');
    expect(checkedMessage(null, now)).toBe('返信はまだ確認していません');
  });
  it('shows expired grant clearly', async () => {
    setup(); vi.mocked(replyStatus).mockResolvedValue({ ok: true, data: { last_checked_at: null, last_imported_at: null, grant_expired: true, interval_seconds: 600 } });
    render(<ReplySyncStatus dealId="700" />);
    expect((await screen.findByText('Gmail の許可が切れています')).closest('p')?.className).toContain('crm-email-failed');
    expect(screen.getByRole('link', { name: 'Gmail の許可を更新する' }).getAttribute('href')).toBe('/api/crm/gmail/authorize');
  });
  it('refreshes activities only when a new import is reported', async () => {
    setup();
    const base = { last_checked_at: '2026-10-11T11:50:00Z', last_imported_at: null, grant_expired: false, interval_seconds: 600 };
    vi.mocked(replyStatus).mockResolvedValue({ ok: true, data: base });
    const changed = vi.fn(); window.addEventListener(EMAIL_CHANGED, changed);
    render(<ReplySyncStatus dealId="700" />);
    await waitFor(() => { expect(replyStatus).toHaveBeenCalled(); });
    window.dispatchEvent(new CustomEvent(EMAIL_CHANGED, { detail: '700' }));
    await waitFor(() => { expect(replyStatus).toHaveBeenCalledTimes(2); });
    expect(changed).toHaveBeenCalledTimes(1);
    vi.mocked(replyStatus).mockResolvedValue({ ok: true, data: { ...base, last_imported_at: '2026-10-11T12:00:00Z' } });
    window.dispatchEvent(new CustomEvent(EMAIL_CHANGED, { detail: '700' }));
    await waitFor(() => { expect(changed).toHaveBeenCalledTimes(3); }); // external event + import refresh
    window.removeEventListener(EMAIL_CHANGED, changed);
  });
});
