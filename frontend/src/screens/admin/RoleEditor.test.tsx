// @vitest-environment happy-dom
// Role editor on the user detail page: POST /api/admin/users/{id}/role (admin only, enforced in Rust).
// fetch is faked; the request the screen sends is checked by value (path, method, body, CSRF header).
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { describeRoleChangeError, RoleEditor, ROLE_OPTIONS } from './AdminScreen';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function stubFetch(res: Response) {
  const fn = vi.fn(() => Promise.resolve(res));
  vi.stubGlobal('fetch', fn);
  return fn;
}

describe('RoleEditor', () => {
  it('offers only admin and general user', () => {
    expect(ROLE_OPTIONS.map((o) => o.value)).toEqual(['admin', 'user']);
  });

  it('disables the button until the choice differs, then posts the role and shows the result', async () => {
    const fetchFn = stubFetch(
      jsonResponse(200, {
        account: { id: 'acc-0001', email: 'hanako@f-a-c.co.jp', role: 'admin' },
        previous_role: 'user',
      }),
    );
    const changed = vi.fn();
    render(<RoleEditor accountId="acc-0001" email="hanako@f-a-c.co.jp" current="user" onChanged={changed} />);
    const button = screen.getByRole('button', { name: '役割を変更' });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('役割'), { target: { value: 'admin' } });
    expect((button as HTMLButtonElement).disabled).toBe(false);
    await act(async () => {
      fireEvent.click(button);
      await Promise.resolve();
    });
    expect(fetchFn).toHaveBeenCalledTimes(1);
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/admin/users/acc-0001/role');
    expect(init.method).toBe('POST');
    expect(init.body).toBe('{"role":"admin"}');
    expect((init.headers as Record<string, string>)['X-Requested-With']).toBe('fetch');
    expect(changed).toHaveBeenCalledWith('admin');
    expect(screen.getByTestId('role-message').textContent).toBe(
      'hanako@f-a-c.co.jp の役割を user から admin に変更しました。',
    );
  });

  it('shows the reason on failure and does not report a change', async () => {
    stubFetch(jsonResponse(409, { error_kind: 'env_admin' }));
    const changed = vi.fn();
    render(<RoleEditor accountId="acc-9" email="boss@f-a-c.co.jp" current="admin" onChanged={changed} />);
    fireEvent.change(screen.getByLabelText('役割'), { target: { value: 'user' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '役割を変更' }));
      await Promise.resolve();
    });
    expect(changed).not.toHaveBeenCalled();
    expect(screen.getByTestId('role-message').textContent).toContain('ADMIN_EMAILS');
  });

  it('shows an old consultant/bpo value as legacy and preselects user', () => {
    render(<RoleEditor accountId="a" email="x@f-a-c.co.jp" current="bpo" onChanged={vi.fn()} />);
    expect(screen.getByTestId('role-legacy').textContent).toContain('「bpo」');
    expect(screen.queryByTestId('role-unknown')).toBeNull();
    expect(screen.getByLabelText<HTMLSelectElement>('役割').value).toBe('user');
  });

  it('shows an unknown stored value as a warning and preselects user', () => {
    render(<RoleEditor accountId="a" email="x@f-a-c.co.jp" current="boss" onChanged={vi.fn()} />);
    expect(screen.getByTestId('role-unknown').textContent).toContain('「boss」');
    expect(screen.getByLabelText<HTMLSelectElement>('役割').value).toBe('user');
  });
});

describe('describeRoleChangeError', () => {
  it('maps every error_kind of the API to its own message', () => {
    const msg = (status: number, error_kind?: string) =>
      describeRoleChangeError(new ApiHttpError(status, error_kind === undefined ? undefined : { error_kind }));
    const texts = [
      msg(403, 'cannot_change_self'),
      msg(409, 'env_admin'),
      msg(400, 'invalid_role'),
      msg(404, 'account_not_found'),
      msg(403),
      msg(502, 'audit_write_failed'),
    ];
    expect(new Set(texts).size).toBe(texts.length);
    expect(texts[0]).toContain('自分自身');
    expect(texts[4]).toContain('管理者のみ');
    expect(describeRoleChangeError(new Error('x'))).toContain('変更できませんでした');
  });
});
