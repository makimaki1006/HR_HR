/// <reference types="vite/client" />
// Fixtures: JSON written by the Rust tests (src/handlers/my/snapshot_tests.rs).
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import myActivityJson from '../../../../tests/fixtures/w8_admin_my/my_activity.json?raw';
import myProfileAuditDisabledJson from '../../../../tests/fixtures/w8_admin_my/my_profile_audit_disabled.json?raw';
import myProfileJson from '../../../../tests/fixtures/w8_admin_my/my_profile.json?raw';
import myProfileNotLinkedJson from '../../../../tests/fixtures/w8_admin_my/my_profile_not_linked.json?raw';
import { ApiHttpError, AuthRequiredError } from '../../api/client';
import type { MyActivityResponse } from '../../generated/MyActivityResponse';
import type { MyProfileResponse } from '../../generated/MyProfileResponse';
import { ActivityView, MY_UA_CHARS, MyStatusBox, ProfileForm, ProfileView } from './MyScreen';
import { FETCH_MARKER_HEADER, FETCH_MARKER_VALUE, postJson } from './postJson';
import { myApiPath, myHref, parseMyRoute } from './route';

const profile = JSON.parse(myProfileJson) as MyProfileResponse;
const activity = JSON.parse(myActivityJson) as MyActivityResponse;
const auditDisabled = JSON.parse(myProfileAuditDisabledJson) as MyProfileResponse;
const notLinked = JSON.parse(myProfileNotLinkedJson) as MyProfileResponse;

function rows(html: string, testId: string): string[][] {
  const table = new RegExp(`<table[^>]*data-testid="${testId}"[^>]*>([\\s\\S]*?)</table>`).exec(html);
  if (!table) throw new Error(`table ${testId} not found`);
  const body = /<tbody>([\s\S]*?)<\/tbody>/.exec(table[1] ?? '')?.[1] ?? '';
  return [...body.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].map((m) =>
    [...(m[1] ?? '').matchAll(/<td[^>]*>([\s\S]*?)<\/td>/g)].map((c) =>
      (c[1] ?? '').replace(/<[^>]+>/g, '').trim(),
    ),
  );
}

const noop = (): undefined => undefined;

describe('ProfileView', () => {
  it('shows the read-only values and pre-fills the form from the JSON', () => {
    if (profile.status !== 'ok') throw new Error('fixture must be ok');
    const html = renderToStaticMarkup(<ProfileView account={profile.account} flash={null} onSaved={noop} />);
    const dds = [...html.matchAll(/<dd[^>]*>([\s\S]*?)<\/dd>/g)].map((m) => m[1] ?? '');
    expect(dds).toEqual(['hanako@f-a-c.co.jp', 'user', '2026-01-05T09:00:00Z', '2026-09-28T01:23:45Z', '12']);
    expect(html).toContain('maxLength="80" name="display_name" value="山田 花子"');
    expect(html).toContain('maxLength="120" name="company" value="F&amp;A &lt;Consulting&gt;"');
    expect(html).not.toContain('data-testid="profile-flash"');
  });

  it('shows the flash line after a save', () => {
    if (profile.status !== 'ok') throw new Error('fixture must be ok');
    const html = renderToStaticMarkup(
      <ProfileView account={profile.account} flash="プロフィールを更新しました" onSaved={noop} />,
    );
    expect(html).toContain('data-testid="profile-flash">プロフィールを更新しました</div>');
  });

  it('renders the two non-ok statuses with the old pages wording', () => {
    expect(auditDisabled).toEqual({ status: 'audit_disabled' });
    expect(notLinked).toEqual({ status: 'not_linked' });
    expect(renderToStaticMarkup(<MyStatusBox status="audit_disabled" />)).toContain(
      '<h1>この機能は現在ご利用いただけません</h1>',
    );
    expect(renderToStaticMarkup(<MyStatusBox status="not_linked" />)).toContain(
      '<h1>アカウントが見つかりません</h1>',
    );
  });
});

describe('ActivityView', () => {
  it('shows both tables with values from the JSON, UA cut to 50 chars', () => {
    if (activity.status !== 'ok') throw new Error('fixture must be ok');
    const html = renderToStaticMarkup(<ActivityView data={activity} />);
    expect(html).toContain('data-testid="activity-email">hanako@f-a-c.co.jp</h1>');
    expect(html).toContain('ご自身の最近の利用履歴 (直近50ログイン / 直近100操作)');
    const s = rows(html, 'my-sessions-table');
    expect(s).toHaveLength(3);
    const ua50 = Array.from(activity.sessions[0]?.user_agent ?? '')
      .slice(0, MY_UA_CHARS)
      .join('');
    expect(ua50).toHaveLength(50);
    expect(s[0]).toEqual(['2099-01-02T09:00:00Z', '成功', ua50]);
    expect(s[1]).toEqual(['2099-01-02T08:00:00Z', '失敗', 'curl/8.0']);
    const a = rows(html, 'my-activities-table');
    expect(a).toHaveLength(4);
    expect(a[1]).toEqual(['2099-01-02T09:20:00Z', 'view_tab', 'tab', '/tab/survey']);
  });
});

describe('route', () => {
  it('defaults to profile', () => {
    expect(parseMyRoute('')).toEqual({ view: 'profile' });
    expect(parseMyRoute('?view=activity')).toEqual({ view: 'activity' });
    expect(parseMyRoute('?view=bogus')).toEqual({ view: 'profile' });
    expect(myHref({ view: 'activity' })).toBe('?view=activity');
    expect(myApiPath({ view: 'profile' })).toBe('/api/my/profile');
    expect(myApiPath({ view: 'activity' })).toBe('/api/my/activity');
  });
});

describe('postJson', () => {
  type FetchMock = ReturnType<typeof vi.fn<typeof fetch>>;
  let fetchMock: FetchMock;
  beforeEach(() => {
    fetchMock = vi.fn<typeof fetch>();
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('POSTs JSON with the fetch marker header and same-origin cookies', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ status: 'ok', account: { id: 'a' } }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const result = await postJson<{ status: string }>('/api/my/profile', { display_name: '花子', company: '' });
    expect(result).toEqual({ ok: true, data: { status: 'ok', account: { id: 'a' } } });
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe('/api/my/profile');
    expect(init?.method).toBe('POST');
    expect(init?.credentials).toBe('same-origin');
    expect(init?.body).toBe('{"display_name":"花子","company":""}');
    expect(init?.headers).toEqual({
      Accept: 'application/json',
      'Content-Type': 'application/json',
      [FETCH_MARKER_HEADER]: FETCH_MARKER_VALUE,
    });
    expect(FETCH_MARKER_VALUE).toBe('fetch');
  });

  it('maps 403 to ApiHttpError and a login redirect to AuthRequiredError', async () => {
    fetchMock.mockResolvedValueOnce(new Response('Forbidden', { status: 403 }));
    const r1 = await postJson('/api/my/profile', {});
    expect(r1.ok).toBe(false);
    if (!r1.ok) {
      expect(r1.error).toBeInstanceOf(ApiHttpError);
      expect((r1.error as ApiHttpError).status).toBe(403);
    }
    const res = new Response('<html>login</html>', { status: 200, headers: { 'content-type': 'text/html' } });
    Object.defineProperty(res, 'redirected', { value: true });
    Object.defineProperty(res, 'url', { value: 'http://localhost:8080/login' });
    fetchMock.mockResolvedValueOnce(res);
    const r2 = await postJson('/api/my/profile', {});
    expect(r2.ok).toBe(false);
    if (!r2.ok) expect(r2.error).toBeInstanceOf(AuthRequiredError);
    const r3 = await postJson('https://evil.example/x', {});
    expect(r3.ok).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});

describe('ProfileForm', () => {
  it('renders the inputs with the account values and the Rust length limits', () => {
    if (profile.status !== 'ok') throw new Error('fixture must be ok');
    const html = renderToStaticMarkup(<ProfileForm account={profile.account} onSaved={noop} />);
    expect(html).toContain('maxLength="80" name="display_name" value="山田 花子"');
    expect(html).toContain('maxLength="120" name="company" value="F&amp;A &lt;Consulting&gt;"');
    expect(html).toContain('<button type="submit" class="w8-button">保存</button>');
  });
});
