// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { JobCopyApp } from './JobCopyApp';
import { jobs } from './data';

const nav = {
  user_email: 'synthetic@example.co.jp', is_admin: false, header_links: [], groups: [],
  items: [{ id: 'job-copy', label: '求人文面', title: null, kind: 'app', href: '/app/job-copy', group: null, hidden: false, hidden_reason: null, hidden_since: null }],
};
const response = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'content-type': 'application/json' } });

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

describe('job copy shared application shell', () => {
  it('uses one common header and active navigation while a protected snapshot fails without fictional jobs', async () => {
    window.history.replaceState(null, '', '/app/job-copy');
    const calls: string[] = [];
    vi.stubGlobal('fetch', vi.fn((input: string) => {
      calls.push(input);
      if (input === '/api/nav') return Promise.resolve(response(nav));
      if (input === '/api/job-copy/moc') return Promise.resolve(response({ code: 'login_required' }, 401));
      throw new Error('Unexpected endpoint in isolated shell test');
    }));
    const { container } = render(<JobCopyApp />);
    const navigation = await screen.findByRole('navigation', { name: 'ダッシュボードナビ' });
    expect(within(navigation).getByRole('link', { name: '求人文面' }).getAttribute('aria-current')).toBe('page');
    expect(container.querySelectorAll('.hr-header')).toHaveLength(1);
    expect(container.querySelectorAll('.jc-topbar')).toHaveLength(0);
    expect(container.querySelector('a')?.getAttribute('href')).toBe('#job-details');
    await waitFor(() => { expect(screen.getByRole('alert').textContent).toContain('ログインが必要です'); });
    expect(screen.getByRole('link', { name: '再ログインする' }).getAttribute('href')).toBe('/login');
    expect(container.querySelectorAll('.jc-job')).toHaveLength(0);
    expect(calls.every(path => path === '/api/nav' || path === '/api/job-copy/moc')).toBe(true);
  });

  it('keeps explicit demo mode inside the shared shell without fetching private snapshots or global geography filters', async () => {
    window.history.replaceState(null, '', '/app/job-copy?demo=1');
    const calls: string[] = [];
    vi.stubGlobal('fetch', vi.fn((input: string) => {
      calls.push(input);
      if (input !== '/api/nav') throw new Error('Demo requested private data or unrelated filters');
      return Promise.resolve(response(nav));
    }));
    const { container } = render(<JobCopyApp />);
    await screen.findByRole('navigation', { name: 'ダッシュボードナビ' });
    expect(container.querySelectorAll('.jc-job')).toHaveLength(jobs.length);
    expect(container.querySelector('.jc-demo')?.textContent).toContain('すべて架空');
    expect(container.querySelectorAll('.hr-header')).toHaveLength(1);
    expect(calls).toEqual(['/api/nav']);
  });
});
