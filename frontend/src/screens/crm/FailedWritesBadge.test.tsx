// @vitest-environment happy-dom
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FAILED_COUNT_PATH, FAILED_LIST_HREF, FAILED_WRITES_REFRESH_MS, FailedWritesBadge } from './FailedWritesBadge';

const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../api/client')>()),
  apiGet: api,
}));

function serve(isAdmin: boolean, failed: () => number | 'error') {
  api.mockImplementation((path: string) => {
    if (path === '/api/nav') return Promise.resolve({ ok: true, data: { user_email: 'a@f-a-c.co.jp', is_admin: isAdmin } });
    if (path === FAILED_COUNT_PATH) {
      const n = failed();
      return Promise.resolve(n === 'error' ? { ok: false, error: new Error('503') } : { ok: true, data: { failed: n } });
    }
    return Promise.reject(new Error(`unexpected ${path}`));
  });
}
const calls = (path: string) => api.mock.calls.filter(c => c[0] === path).length;

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); vi.resetAllMocks(); });

describe('FailedWritesBadge', () => {
  it('管理者で 3 件なら件数と一覧へのリンクを出す', async () => {
    serve(true, () => 3);
    render(<FailedWritesBadge />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    const a = screen.getByTestId('failed-writes-badge');
    expect(a.textContent).toBe('HubSpot に保存できなかった操作が 3 件あります');
    expect(a.getAttribute('href')).toBe(FAILED_LIST_HREF);
  });

  it('0 件なら何も出さない', async () => {
    serve(true, () => 0);
    render(<FailedWritesBadge />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(screen.queryByTestId('failed-writes-badge')).toBeNull();
  });

  it('管理者以外は件数の API を呼ばず、何も出さない', async () => {
    serve(false, () => 3);
    render(<FailedWritesBadge />);
    await act(async () => { await vi.advanceTimersByTimeAsync(FAILED_WRITES_REFRESH_MS * 2); });
    expect(calls(FAILED_COUNT_PATH)).toBe(0);
    expect(screen.queryByTestId('failed-writes-badge')).toBeNull();
  });

  it('取れなければ何も出さない (エラーを出さない)', async () => {
    serve(true, () => 'error');
    const { container } = render(<FailedWritesBadge />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(container.textContent).toBe('');
  });

  it('2 分おきに取り直し、タブが隠れている間は止める', async () => {
    let n = 1;
    serve(true, () => n);
    render(<FailedWritesBadge />);
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(calls(FAILED_COUNT_PATH)).toBe(1);
    n = 4;
    await act(async () => { await vi.advanceTimersByTimeAsync(FAILED_WRITES_REFRESH_MS); });
    expect(calls(FAILED_COUNT_PATH)).toBe(2);
    expect(screen.getByTestId('failed-writes-badge').textContent).toContain('4 件');
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    await act(async () => { await vi.advanceTimersByTimeAsync(FAILED_WRITES_REFRESH_MS * 3); });
    expect(calls(FAILED_COUNT_PATH)).toBe(2);
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
  });
});
