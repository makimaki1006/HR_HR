// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ApplicantCalendarStatus } from '../../generated/ApplicantCalendarStatus';
import { CalendarView, jstText } from './CalendarPanel';
import { adminApiPath, adminHref, parseAdminRoute } from './route';

const post = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/client')>()),
  apiPost: post,
}));
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const base: ApplicantCalendarStatus = {
  configured: true,
  enabled: true,
  account: 'calendar-admin@example.com',
  is_account: false,
  state: 'granted',
  granted_at: '2026-10-29T01:02:03Z',
  kinds: ['面接', '入社', '内定'],
  last_sync_at: '2026-10-29T03:04:00Z',
  last_created: 3,
  last_updated: 1,
  last_deleted: 2,
  last_errors: 0,
  errors_since_start: 4,
  problem: null,
};

describe('CalendarView', () => {
  it('許可の状態と最後の同期を日本時間で出す', () => {
    render(<CalendarView data={base} onChanged={() => undefined} />);
    expect(screen.getByTestId('calendar-state').textContent).toBe('許可済み（2026/10/29 10:02 に許可）');
    expect(screen.getByTestId('calendar-last-sync').textContent).toBe('2026/10/29 12:04');
    expect(screen.getByTestId('calendar-last-counts').textContent).toBe('追加 3 件・変更 1 件・削除 2 件・書けなかった 0 件');
    expect(screen.getByTestId('calendar-errors').textContent).toBe('4 件（アプリを起動してからの合計）');
    expect(document.body.textContent).toContain('面接・入社・内定');
    // 本人でなければ許可のボタンは出さない
    expect(screen.queryByText('許可し直す')).toBeNull();
    // 開発者用語を出さない
    for (const word of ['OAuth', 'token', 'API', 'scope']) {
      expect(document.body.textContent).not.toContain(word);
    }
  });

  it('切れていれば赤で知らせ、本人には許可のリンクを出す', () => {
    render(<CalendarView data={{ ...base, state: 'expired', is_account: true }} onChanged={() => undefined} />);
    expect(screen.getByTestId('calendar-expired').textContent).toContain('許可が切れています');
    const link = screen.getByText('管理用カレンダーへの書き込みを許可する');
    expect(link.getAttribute('href')).toBe('/api/admin/applicant-calendar/authorize');
  });

  it('設定が無ければその旨だけ出す', () => {
    render(<CalendarView data={{ ...base, configured: false, account: null }} onChanged={() => undefined} />);
    expect(screen.getByTestId('calendar-not-configured')).toBeTruthy();
    expect(screen.queryByTestId('calendar-state')).toBeNull();
  });

  it('取り消すと revoke を POST する', async () => {
    post.mockResolvedValue({ ok: true, data: { state: 'none', google_revocation_pending: false } });
    const onChanged = vi.fn();
    render(<CalendarView data={base} onChanged={onChanged} />);
    fireEvent.click(screen.getByText('許可を取り消す'));
    await waitFor(() => { expect(onChanged).toHaveBeenCalled(); });
    expect(post).toHaveBeenCalledWith('/api/admin/applicant-calendar/revoke', {});
  });
});

describe('route', () => {
  it('view=calendar を往復する', () => {
    expect(parseAdminRoute('?view=calendar')).toEqual({ view: 'calendar' });
    expect(adminHref({ view: 'calendar' })).toBe('?view=calendar');
    expect(adminApiPath({ view: 'calendar' })).toBe('/api/admin/applicant-calendar');
    expect(jstText(null)).toBe('');
  });
});
