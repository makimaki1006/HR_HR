// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from './AppShell';
import { useFilters } from './filters';
import type { FiltersCurrent, NavItem, NavResponse } from './types';

const redirectToLogin = vi.hoisted(() => vi.fn());
vi.mock('./navigation', () => ({ redirectToLogin }));

const item = (
  id: string,
  label: string,
  kind: NavItem['kind'],
  href: string,
  group: string | null = null,
  title: string | null = null,
): NavItem => ({
  id,
  label,
  title,
  kind,
  href,
  group,
  hidden: false,
  hidden_reason: null,
  hidden_since: null,
});

const NAV: NavResponse = {
  user_email: 'sales@example.co.jp',
  is_admin: false,
  header_links: [
    item('settings', '設定', 'page', '/my/profile'),
    item('logout', 'ログアウト', 'page', '/logout'),
  ],
  items: [
    item('survey', '媒体分析', 'legacy_tab', '/?tab=/tab/survey'),
    item('recruitment-diag', '採用診断', 'app', '/app/recruitment-diag'),
    item('jobmap', '地図', 'legacy_tab', '/?tab=/tab/jobmap', 'explore'),
    item('company', '企業検索', 'legacy_tab', '/?tab=/tab/company', 'explore'),
    item('consulting', 'コンサルKPI', 'page', '/consulting', null, 'コンサル用'),
  ],
  groups: [{ id: 'explore', label: '調べる' }],
};

const CURRENT: FiltersCurrent = {
  prefecture: '',
  municipality: '',
  job_types: [],
  industry_raws: [],
};

interface Call {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: string | null;
}
let calls: Call[];

const jsonRes = (body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const htmlRes = (body: string): Response =>
  new Response(body, { status: 200, headers: { 'content-type': 'text/html' } });

type Override = (url: string, init?: RequestInit) => Response | undefined;

function mockFetch(
  opts: { nav?: NavResponse; current?: FiltersCurrent; override?: Override } = {},
): void {
  const nav = opts.nav ?? NAV;
  const current = opts.current ?? CURRENT;
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
      calls.push({
        url,
        method: init?.method ?? 'GET',
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: typeof init?.body === 'string' ? init.body : null,
      });
      const overridden = opts.override?.(url, init);
      if (overridden !== undefined) return Promise.resolve(overridden);
      if (url === '/api/nav') return Promise.resolve(jsonRes(nav));
      if (url === '/api/filters/current') return Promise.resolve(jsonRes(current));
      if (url === '/api/prefectures') {
        return Promise.resolve(
          htmlRes('<option value="東京都">東京都</option>\n<option value="大阪府">大阪府</option>'),
        );
      }
      if (url.startsWith('/api/municipalities_cascade')) {
        return Promise.resolve(
          htmlRes('<option value="新宿区" data-citycode="13104">新宿区</option>'),
        );
      }
      if (url.startsWith('/api/set_')) return Promise.resolve(htmlRes('OK'));
      return Promise.resolve(new Response('nf', { status: 404 }));
    }),
  );
}

const setUrl = (search: string): void => {
  window.history.replaceState(null, '', `/app/dummy${search}`);
};
const posts = (): Call[] => calls.filter((c) => c.method === 'POST');

beforeEach(() => {
  calls = [];
  redirectToLogin.mockClear();
  setUrl('');
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('AppShell header / nav', () => {
  it('shows email and header links in order, and no admin link for a non-admin', async () => {
    mockFetch();
    render(
      <AppShell screen="recruitment-diag">
        <p>child</p>
      </AppShell>,
    );
    await screen.findByText('ログイン: sales@example.co.jp');
    const links = within(screen.getByTestId('header-links')).getAllByRole('link');
    expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
      ['設定', '/my/profile'],
      ['ログアウト', '/logout'],
    ]);
    expect(screen.queryByText('管理')).toBeNull();
  });

  it('shows the admin link when header_links carries it (admin)', async () => {
    mockFetch({
      nav: {
        ...NAV,
        is_admin: true,
        header_links: [item('admin', '管理', 'page', '/admin/usage', null, '利用状況'), ...NAV.header_links],
      },
    });
    render(
      <AppShell screen="x">
        <p>child</p>
      </AppShell>,
    );
    const admin = await screen.findByRole('link', { name: '管理' });
    expect(admin.getAttribute('href')).toBe('/admin/usage');
    expect(admin.getAttribute('title')).toBe('利用状況');
  });

  it('renders items in order with hrefs; a group is one button at its first member', async () => {
    mockFetch();
    render(
      <AppShell screen="nothing">
        <p>child</p>
      </AppShell>,
    );
    const nav = await screen.findByRole('navigation', { name: 'ダッシュボードナビ' });
    const entries = Array.from(nav.children).map((el) => [
      el.tagName,
      el.textContent,
      el.getAttribute('href'),
    ]);
    expect(entries).toEqual([
      ['A', '媒体分析', '/?tab=/tab/survey'],
      ['A', '採用診断', '/app/recruitment-diag'],
      ['BUTTON', '調べる ▾', null],
      ['A', 'コンサルKPI', '/consulting'],
    ]);
    expect(screen.queryByText('地図')).toBeNull();
    expect(within(nav).getByRole('link', { name: 'コンサルKPI' }).getAttribute('title')).toBe(
      'コンサル用',
    );
  });

  it('opens the sub nav with the group items on click and closes on the second click', async () => {
    mockFetch();
    render(
      <AppShell screen="nothing">
        <p>child</p>
      </AppShell>,
    );
    const btn = await screen.findByRole('button', { name: /調べる/ });
    expect(btn.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(btn);
    const sub = screen.getByRole('navigation', { name: '調べる' });
    expect(
      within(sub)
        .getAllByRole('link')
        .map((a) => [a.textContent, a.getAttribute('href')]),
    ).toEqual([
      ['地図', '/?tab=/tab/jobmap'],
      ['企業検索', '/?tab=/tab/company'],
    ]);
    expect(btn.getAttribute('aria-expanded')).toBe('true');
    fireEvent.click(btn);
    expect(screen.queryByRole('navigation', { name: '調べる' })).toBeNull();
  });

  it('marks only the kind=app item whose href is /app/<screen> as current', async () => {
    mockFetch();
    render(
      <AppShell screen="recruitment-diag">
        <p>child</p>
      </AppShell>,
    );
    const active = await screen.findByRole('link', { name: '採用診断' });
    expect(active.getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('link', { name: '媒体分析' }).getAttribute('aria-current')).toBeNull();
    expect(
      screen.getByRole('link', { name: 'コンサルKPI' }).getAttribute('aria-current'),
    ).toBeNull();
  });

  it('does not treat a legacy_tab with the same href as active', async () => {
    mockFetch({
      nav: { ...NAV, items: [item('a', 'A画面', 'legacy_tab', '/app/dummy')] },
    });
    render(
      <AppShell screen="dummy">
        <p>child</p>
      </AppShell>,
    );
    const link = await screen.findByRole('link', { name: 'A画面' });
    expect(link.getAttribute('aria-current')).toBeNull();
  });

  it('opens the group holding the active app item by default', async () => {
    mockFetch({
      nav: {
        ...NAV,
        items: [
          item('x', '画面X', 'app', '/app/x', 'g'),
          item('y', '画面Y', 'app', '/app/y', 'g'),
        ],
        groups: [{ id: 'g', label: 'グループ' }],
      },
    });
    render(
      <AppShell screen="y">
        <p>child</p>
      </AppShell>,
    );
    const sub = await screen.findByRole('navigation', { name: 'グループ' });
    expect(within(sub).getByRole('link', { name: '画面Y' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(within(sub).getByRole('link', { name: '画面X' }).getAttribute('aria-current')).toBeNull();
  });

  it('does not render hidden items (top level or inside a group) but keeps visible ones', async () => {
    const hiddenTop: NavItem = {
      ...item('insight', '総合診断', 'legacy_tab', '/?tab=/tab/insight'),
      hidden: true,
      hidden_reason: 'not in use',
      hidden_since: '2026-09-29',
    };
    const hiddenInGroup: NavItem = {
      ...item('trend', 'トレンド', 'legacy_tab', '/?tab=/tab/trend', 'explore'),
      hidden: true,
      hidden_reason: null,
      hidden_since: null,
    };
    mockFetch({ nav: { ...NAV, items: [...NAV.items, hiddenTop, hiddenInGroup] } });
    render(
      <AppShell screen="company">
        <p>child</p>
      </AppShell>,
    );
    await screen.findByText('媒体分析');
    expect(document.querySelector('[data-nav-id="insight"]')).toBeNull();
    expect(document.querySelector('[data-nav-id="trend"]')).toBeNull();
    expect(document.querySelector('[data-nav-id="survey"]')).not.toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /調べる/ }));
    const sub = await screen.findByRole('navigation', { name: '調べる' });
    expect(within(sub).getAllByRole('link').map((a) => a.textContent)).toEqual([
      '地図',
      '企業検索',
    ]);
  });

  it('still renders children when /api/nav fails (graceful)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(new Response('x', { status: 500 }))),
    );
    render(
      <AppShell screen="dummy">
        <p>child body</p>
      </AppShell>,
    );
    expect(await screen.findByText('child body')).toBeTruthy();
    expect(screen.queryByRole('navigation')).toBeNull();
    expect(redirectToLogin).not.toHaveBeenCalled();
  });

  it('goes to /login when /api/nav answers with the login page (unauthenticated)', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(htmlRes('<html><form method="post" action="/login"></form></html>'))),
    );
    render(
      <AppShell screen="dummy">
        <p>child body</p>
      </AppShell>,
    );
    await waitFor(() => {
      expect(redirectToLogin).toHaveBeenCalled();
    });
  });
});

function Probe() {
  const { filters } = useFilters();
  return <p data-testid="probe">{JSON.stringify(filters)}</p>;
}

type Ctx = ReturnType<typeof useFilters>;
let ctx: Ctx | null = null;
function CtxProbe() {
  ctx = useFilters();
  return <p data-testid="probe">{JSON.stringify(ctx.filters)}</p>;
}
const getCtx = (): Ctx => {
  if (!ctx) throw new Error('no context');
  return ctx;
};
const errRes = (status: number): Response => new Response('boom', { status });
const probed = (el: HTMLElement): FiltersCurrent => JSON.parse(el.textContent) as FiltersCurrent;

describe('AppShell filters', () => {
  it('URL query wins: set_prefecture then set_municipality are posted (form body + X-Requested-With)', async () => {
    mockFetch({ current: { ...CURRENT, prefecture: '大阪府', municipality: '北区' } });
    setUrl('?pref=%E6%9D%B1%E4%BA%AC%E9%83%BD&muni=%E6%96%B0%E5%AE%BF%E5%8C%BA');
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    const probe = await screen.findByTestId('probe');
    expect(probed(probe)).toEqual({
      prefecture: '東京都',
      municipality: '新宿区',
      job_types: [],
      industry_raws: [],
    });
    const p = posts();
    expect(p.map((c) => [c.url, c.body])).toEqual([
      ['/api/set_prefecture', 'prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD'],
      ['/api/set_municipality', 'municipality=%E6%96%B0%E5%AE%BF%E5%8C%BA'],
    ]);
    for (const c of p) {
      expect(c.headers['X-Requested-With']).toBe('fetch');
      expect(c.headers['Content-Type']).toBe('application/x-www-form-urlencoded;charset=UTF-8');
    }
  });

  it('without a query it takes /api/filters/current and posts nothing', async () => {
    mockFetch({
      current: {
        prefecture: '大阪府',
        municipality: '北区',
        job_types: ['医療'],
        industry_raws: ['病院'],
      },
    });
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    const probe = await screen.findByTestId('probe');
    expect(probed(probe)).toEqual({
      prefecture: '大阪府',
      municipality: '北区',
      job_types: ['医療'],
      industry_raws: ['病院'],
    });
    expect(posts()).toEqual([]);
  });

  it('a query with only ?pref= clears the session municipality', async () => {
    mockFetch({ current: { ...CURRENT, prefecture: '大阪府', municipality: '北区' } });
    setUrl('?pref=%E6%9D%B1%E4%BA%AC%E9%83%BD');
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    const probe = await screen.findByTestId('probe');
    expect(probed(probe).municipality).toBe('');
    expect(posts().map((c) => c.url)).toEqual(['/api/set_prefecture']);
  });

  it('?ind= and ?jt= are posted as comma-joined set_industry_filter', async () => {
    mockFetch();
    setUrl('?ind=a%2Cb&jt=x');
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    expect(posts().map((c) => [c.url, c.body])).toEqual([
      ['/api/set_industry_filter', 'job_types=x&industry_raws=a%2Cb'],
    ]);
  });

  it('changing prefecture then municipality posts set_* and rewrites the URL query', async () => {
    mockFetch();
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    await screen.findByRole('option', { name: '東京都' });
    fireEvent.change(screen.getByLabelText(/都道府県/), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(posts().map((c) => [c.url, c.body])).toEqual([
        ['/api/set_prefecture', 'prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD'],
      ]);
    });
    await screen.findByRole('option', { name: '新宿区' });
    expect(new URLSearchParams(window.location.search).get('pref')).toBe('東京都');
    expect(
      calls.some(
        (c) => c.url === '/api/municipalities_cascade?prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD',
      ),
    ).toBe(true);

    fireEvent.change(screen.getByLabelText('市区町村'), { target: { value: '新宿区' } });
    await waitFor(() => {
      expect(posts()[1]?.body).toBe('municipality=%E6%96%B0%E5%AE%BF%E5%8C%BA');
    });
    expect(new URLSearchParams(window.location.search).get('muni')).toBe('新宿区');
  });

  it('does not render children before the initial filter sync completes', async () => {
    mockFetch();
    render(
      <AppShell screen="dummy" filters>
        <p>screen body</p>
      </AppShell>,
    );
    expect(screen.queryByText('screen body')).toBeNull();
    expect(await screen.findByText('screen body')).toBeTruthy();
  });

  it('rolls state and URL back and shows an error when set_prefecture answers 500', async () => {
    mockFetch({
      current: { ...CURRENT, prefecture: '大阪府' },
      override: (url) => (url === '/api/set_prefecture' ? errRes(500) : undefined),
    });
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    await screen.findByRole('option', { name: '東京都' });
    const select = screen.getByLabelText(/都道府県/) as HTMLSelectElement;
    expect(select.value).toBe('大阪府');
    fireEvent.change(select, { target: { value: '東京都' } });
    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toContain('HTTP 500');
    await waitFor(() => {
      expect((screen.getByLabelText(/都道府県/) as HTMLSelectElement).value).toBe('大阪府');
    });
    expect(probed(screen.getByTestId('probe')).prefecture).toBe('大阪府');
    expect(new URLSearchParams(window.location.search).get('pref')).toBe('大阪府');
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('rolls back on a network failure and clears the error on the next successful change', async () => {
    let failNext = true;
    mockFetch({
      override: (url) => {
        if (url === '/api/set_prefecture' && failNext) {
          failNext = false;
          throw new TypeError('Failed to fetch');
        }
        return undefined;
      },
    });
    render(
      <AppShell screen="dummy" filters>
        <Probe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    await screen.findByRole('option', { name: '東京都' });
    fireEvent.change(screen.getByLabelText(/都道府県/), { target: { value: '東京都' } });
    await screen.findByRole('alert');
    expect(probed(screen.getByTestId('probe')).prefecture).toBe('');
    expect(new URLSearchParams(window.location.search).get('pref')).toBeNull();
    fireEvent.change(screen.getByLabelText(/都道府県/), { target: { value: '東京都' } });
    await waitFor(() => {
      expect(screen.queryByRole('alert')).toBeNull();
    });
    expect(probed(screen.getByTestId('probe')).prefecture).toBe('東京都');
  });

  it('shows an error and does not become ready when /api/filters/current answers 404 or 500', async () => {
    for (const status of [404, 500]) {
      mockFetch({
        override: (url) => (url === '/api/filters/current' ? errRes(status) : undefined),
      });
      const { unmount } = render(
        <AppShell screen="dummy" filters>
          <p>screen body</p>
        </AppShell>,
      );
      const alert = await screen.findByRole('alert');
      expect(alert.textContent).toContain(`HTTP ${String(status)}`);
      expect(screen.queryByText('screen body')).toBeNull();
      expect((screen.getByLabelText(/都道府県/) as HTMLSelectElement).disabled).toBe(true);
      unmount();
    }
  });

  it('a failed set_municipality restores the previous municipality; a dependent queued call is dropped', async () => {
    mockFetch({
      override: (url, init) => {
        if (url === '/api/set_municipality' && String(init?.body).includes('%E6%96%B0%E5%AE%BF')) {
          return errRes(500);
        }
        return undefined;
      },
    });
    render(
      <AppShell screen="dummy" filters>
        <CtxProbe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    await act(async () => {
      await getCtx().setPrefecture('東京都');
    });
    await act(async () => {
      const first = getCtx().setMunicipality('新宿区'); // fails
      const second = getCtx().setMunicipality('渋谷区'); // depends on the failed one: dropped
      await Promise.all([first, second]);
    });
    expect(posts().map((c) => c.body)).toEqual([
      'prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD',
      'municipality=%E6%96%B0%E5%AE%BF%E5%8C%BA',
    ]);
    expect(probed(screen.getByTestId('probe'))).toMatchObject({
      prefecture: '東京都',
      municipality: '',
    });
    expect(getCtx().error).toContain('HTTP 500');
  });

  it('serialises set_* calls: prefecture POST arrives before municipality even when it is slow', async () => {
    let releasePref: () => void = () => undefined;
    const prefGate = new Promise<void>((r) => {
      releasePref = r;
    });
    const arrival: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url =
          typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
        calls.push({
          url,
          method: init?.method ?? 'GET',
          headers: {},
          body: typeof init?.body === 'string' ? init.body : null,
        });
        if (url === '/api/filters/current') return jsonRes(CURRENT);
        if (url === '/api/nav') return jsonRes(NAV);
        if (url === '/api/prefectures') return htmlRes('<option value="東京都">東京都</option>');
        if (url.startsWith('/api/municipalities_cascade')) {
          return htmlRes('<option value="新宿区">新宿区</option>');
        }
        if (url === '/api/set_prefecture') {
          await prefGate;
          arrival.push('prefecture');
          return htmlRes('OK');
        }
        if (url === '/api/set_municipality') {
          arrival.push('municipality');
          return htmlRes('OK');
        }
        return new Response('nf', { status: 404 });
      }),
    );
    render(
      <AppShell screen="dummy" filters>
        <CtxProbe />
      </AppShell>,
    );
    await screen.findByTestId('probe');
    let both: Promise<unknown> = Promise.resolve();
    act(() => {
      const p = getCtx().setPrefecture('東京都');
      const m = getCtx().setMunicipality('新宿区');
      both = Promise.all([p, m]);
    });
    // The municipality POST is not sent while the prefecture POST is pending.
    await new Promise((r) => setTimeout(r, 20));
    expect(posts().map((c) => c.url)).toEqual(['/api/set_prefecture']);
    expect(getCtx().syncing).toBe(true);
    // The municipality list is not requested before set_prefecture finishes (legacy order).
    expect(calls.some((c) => c.url.startsWith('/api/municipalities_cascade'))).toBe(false);
    expect((screen.getByLabelText('市区町村') as HTMLSelectElement).disabled).toBe(true);
    await act(async () => {
      releasePref();
      await both;
    });
    expect(arrival).toEqual(['prefecture', 'municipality']);
    expect(probed(screen.getByTestId('probe'))).toMatchObject({
      prefecture: '東京都',
      municipality: '新宿区',
    });
    await waitFor(() => {
      expect(calls.some((c) => c.url.startsWith('/api/municipalities_cascade'))).toBe(true);
    });
    expect(getCtx().syncing).toBe(false);
    expect(new URLSearchParams(window.location.search).get('muni')).toBe('新宿区');
  });
});
