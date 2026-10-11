// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { apiGet } from '../../api/client';
import type { EmailTrackingSummary } from '../../generated/EmailTrackingSummary';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import { ActivityCard } from './ActivityCard';
import { EmailTracking } from './EmailTracking';

vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
const get = vi.mocked(apiGet);
afterEach(() => { cleanup(); vi.resetAllMocks(); });
const counts = (count: number, first: string | null = null, last: string | null = null) => ({ count, first_at: first, last_at: last });
const data: EmailTrackingSummary = {
  opens: counts(2, '2026-10-12T04:00:00Z', '2026-10-12T05:03:00Z'), clicks: counts(1),
  automatic_opens: counts(3), automatic_clicks: counts(2),
  links: [{ link_no: 1, url: 'https://example.invalid/guide', clicks: counts(1), automatic_clicks: counts(2) }],
};
const a: WorkspaceActivity = { id: '999', kind: 'email', timestamp: '2026-10-12T03:00:00Z', title: 'ご案内（架空）', body: '資料をご覧ください。', direction: 'EMAIL', status: 'SENT', duration_ms: null, owner_id: null, source: null, via: 'deal', via_id: '100' };

describe('メールカードの計測', () => {
  it('通常の具体的な件数と日本時間、自動の可能性を分けて表示する', async () => {
    get.mockResolvedValue({ ok: true, data });
    render(<ul><ActivityCard a={a} ownerNames={new Map()} /></ul>);
    await waitFor(() => { expect(screen.getByTestId('email-tracking').textContent).toContain('開封 2 回（最後 10/12 14:03）・クリック 1 回'); });
    expect(screen.getByText('自動の可能性：開封 3 回・クリック 2 回')).toBeTruthy();
    expect(screen.getByText('最初の開封 10/12 13:00・最後の開封 10/12 14:03')).toBeTruthy();
    expect(screen.getByText('https://example.invalid/guide')).toBeTruthy();
    expect(get.mock.calls[0]?.[0]).toBe('/api/crm/emails/999/tracking');
  });
  it('過去メール・無効な機能の204応答では何も追加しない', async () => {
    get.mockRejectedValue(new Error('HTTP 204'));
    render(<ul><ActivityCard a={a} ownerNames={new Map()} /></ul>);
    await waitFor(() => { expect(get).toHaveBeenCalledOnce(); });
    expect(screen.queryByTestId('email-tracking')).toBeNull();
    expect(screen.getByTestId('activity-title').textContent).toBe('ご案内（架空）');
  });
  it('保存済みだが開封されていないメールは0回を表示する', async () => {
    get.mockResolvedValue({ ok: true, data: { ...data, opens: counts(0), clicks: counts(0), automatic_opens: counts(0), automatic_clicks: counts(0), links: [] } });
    render(<EmailTracking emailId="999" />);
    await waitFor(() => { expect(screen.getByTestId('email-tracking').textContent).toContain('開封 0 回・クリック 0 回'); });
    expect(screen.queryByText(/自動の可能性：/)).toBeNull();
  });
  it('メモやサンプルIDでは計測を問い合わせない', () => {
    render(<ul><ActivityCard a={{ ...a, kind: 'note' }} ownerNames={new Map()} /><ActivityCard a={{ ...a, id: 'sample-email-1' }} ownerNames={new Map()} /></ul>);
    expect(get).not.toHaveBeenCalled();
  });
  it('メール切り替え時に前のメールの計測を表示しない', async () => {
    get.mockResolvedValueOnce({ ok: true, data }).mockRejectedValueOnce(new Error('HTTP 204'));
    const { rerender } = render(<EmailTracking emailId="999" />);
    await waitFor(() => { expect(screen.getByTestId('email-tracking')).toBeTruthy(); });
    rerender(<EmailTracking emailId="888" />);
    expect(screen.queryByTestId('email-tracking')).toBeNull();
    await waitFor(() => { expect(get).toHaveBeenCalledTimes(2); });
  });
  it('認可拒否や取得失敗で件数を推測して表示しない', async () => {
    get.mockRejectedValue(new Error('HTTP 403'));
    render(<EmailTracking emailId="999" />);
    await waitFor(() => { expect(get).toHaveBeenCalledOnce(); });
    expect(screen.queryByTestId('email-tracking')).toBeNull();
  });
});

it('自分の送信メールを読むときは計測画像を読み込まず、元のリンクを開く', async () => {
  const { sanitizeHtml } = await import('./activityHtml');
  const id = 'a'.repeat(32);
  const safe = sanitizeHtml(`<p>資料</p><img src="https://hr.invalid/t/o/${id}.gif" data-hr-email-open="1"><a href="https://hr.invalid/t/c/${id}/1" data-hr-email-url="https://example.invalid/guide">資料を見る</a>`);
  expect(safe).not.toContain('/t/o/');
  expect(safe).not.toContain('/t/c/');
  expect(safe).toContain('href="https://example.invalid/guide"');
  expect(safe).toContain('noopener noreferrer');
});
