/// <reference types="vite/client" />
// Fixtures are the JSON written by the Rust tests (tests/fixtures/w8_admin_my/*.json, from
// src/handlers/admin/snapshot_tests.rs) so the React screen is checked against real Rust output,
// not hand-written data. The same fixture produced the HTML snapshots of the old pages.
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import adminLoginFailuresJson from '../../../../tests/fixtures/w8_admin_my/admin_login_failures.json?raw';
import adminUsage30dJson from '../../../../tests/fixtures/w8_admin_my/admin_usage_30d.json?raw';
import adminUsage7dEmptyJson from '../../../../tests/fixtures/w8_admin_my/admin_usage_7d_empty.json?raw';
import adminUserDetailJson from '../../../../tests/fixtures/w8_admin_my/admin_user_detail.json?raw';
import adminUsersJson from '../../../../tests/fixtures/w8_admin_my/admin_users.json?raw';
import { ApiHttpError, AuthRequiredError } from '../../api/client';
import type { AdminLoginFailuresResponse } from '../../generated/AdminLoginFailuresResponse';
import type { AdminUsageResponse } from '../../generated/AdminUsageResponse';
import type { AdminUserDetailResponse } from '../../generated/AdminUserDetailResponse';
import type { AdminUsersResponse } from '../../generated/AdminUsersResponse';
import {
  ADMIN_UA_CHARS,
  AdminNav,
  ErrorBox,
  LoginFailuresView,
  UsageView,
  UserDetailView,
  UsersListView,
} from './AdminScreen';
import { truncateChars } from './format';
import { adminApiPath, adminHref, parseAdminRoute } from './route';
import { describeApiError } from './useApiGet';

const users = JSON.parse(adminUsersJson) as AdminUsersResponse;
const detail = JSON.parse(adminUserDetailJson) as AdminUserDetailResponse;
const failures = JSON.parse(adminLoginFailuresJson) as AdminLoginFailuresResponse;
const usage30 = JSON.parse(adminUsage30dJson) as AdminUsageResponse;
const usage7Empty = JSON.parse(adminUsage7dEmptyJson) as AdminUsageResponse;

/** Cell texts of every row in the tbody of the table with the given data-testid. */
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

describe('UsersListView', () => {
  it('shows the count and one row per account with the same values as the JSON', () => {
    const html = renderToStaticMarkup(<UsersListView data={users} />);
    expect(users.accounts).toHaveLength(3);
    expect(html).toContain('<h1 class="w8-h1">ユーザー一覧 (3 件)</h1>');
    const r = rows(html, 'users-table');
    expect(r).toHaveLength(3);
    // 山田 花子: values straight from the fixture, company escaped by React
    expect(r[0]).toEqual([
      'hanako@f-a-c.co.jp',
      '山田 花子',
      'F&amp;A &lt;Consulting&gt;',
      'user',
      '12',
      '2026-09-28T01:23:45Z',
      '2026-01-05T09:00:00Z',
      '',
    ]);
    // empty name/company -> "-" like the Rust page
    expect(r[1]).toEqual([
      'jiro@client.example',
      '-',
      '-',
      'user',
      '3',
      '2026-09-20T10:00:00Z',
      '2026-03-01T00:00:00Z',
      '',
    ]);
    // disabled admin -> badge text
    expect(r[2]?.[3]).toBe('admin');
    expect(r[2]?.[4]).toBe('45');
    expect(r[2]?.[7]).toBe('無効');
    expect(html).toContain('href="?view=user&amp;id=acc-0001"');
    expect(html).toContain('class="w8-badge w8-badge-admin"');
    expect(html).not.toContain('<Consulting>');
  });
});

describe('UserDetailView', () => {
  it('shows profile, the four 30-day KPIs, and both tables from the JSON', () => {
    const html = renderToStaticMarkup(<UserDetailView data={detail} />);
    expect(html).toContain('data-testid="detail-email">hanako@f-a-c.co.jp</h2>');
    expect(detail.kpi_30d).toEqual({ login_ok: 1, login_fail: 1, activity: 3, company_views: 2 });
    expect(html).toContain('data-testid="kpi-login-ok">1</div>');
    expect(html).toContain('data-testid="kpi-login-fail">1</div>');
    expect(html).toContain('data-testid="kpi-activity">3</div>');
    expect(html).toContain('data-testid="kpi-company-views">2</div>');
    expect(html).toContain('ログイン履歴 (3 件)');
    expect(html).toContain('操作履歴 (4 件)');
    expect(html).toContain('<span class="w8-green">有効</span>');

    const s = rows(html, 'sessions-table');
    expect(s).toHaveLength(3);
    const ua40 = truncateChars(detail.sessions[0]?.user_agent ?? '', ADMIN_UA_CHARS);
    expect(ua40).toHaveLength(40);
    expect(s[0]).toEqual(['2099-01-02T09:00:00Z', '成功', 'password_internal', 'ab12cd34ef56', ua40, '']);
    expect(s[1]).toEqual(['2099-01-02T08:00:00Z', '失敗', 'password', 'ab12cd34ef56', 'curl/8.0', 'wrong_password']);
    expect(html).not.toContain(detail.sessions[0]?.user_agent ?? 'never');

    const a = rows(html, 'activities-table');
    expect(a).toHaveLength(4);
    expect(a[0]).toEqual(['2099-01-02T09:30:00Z', 'view_company_profile', 'company', '1234567890123']);
    expect(a[3]).toEqual(['2000-01-01T00:00:00Z', 'login', '', '']);
  });
});

describe('LoginFailuresView', () => {
  it('lists failures newest first with the UA cut to 40 chars', () => {
    const html = renderToStaticMarkup(<LoginFailuresView data={failures} />);
    expect(html).toContain('ログイン失敗ログ (2 件)');
    expect(html).toContain('同一 ip_hash の連続失敗は不正アクセスの可能性');
    const r = rows(html, 'failures-table');
    expect(r).toHaveLength(2);
    expect(r[0]).toEqual([
      '2026-09-28T00:00:10Z',
      'attacker@evil.example',
      'invalid_domain',
      'ff00ff00ff00',
      'python-requests/2.32',
    ]);
    expect(r[1]?.[4]).toHaveLength(40);
  });
});

describe('UsageView', () => {
  it('shows Japanese labels and counts from the JSON and highlights the active period', () => {
    const html = renderToStaticMarkup(<UsageView data={usage30} />);
    expect(html).toContain('class="w8-pill w8-pill-active">直近30日</a>');
    expect(html).toContain('href="?view=usage&amp;days=7" class="w8-pill">直近7日</a>');
    expect(rows(html, 'usage-by-event')).toEqual([
      ['タブを開く: 媒体分析', '42', '2026-08-10T09:00:00Z'],
      ['CSV取込', '7', '2026-08-10T09:00:00Z'],
      ['mystery_event', '1', '2026-08-10T09:00:00Z'],
    ]);
    expect(rows(html, 'usage-by-account')).toEqual([
      ['hanako@f-a-c.co.jp', '49', '2026-08-10T09:00:00Z'],
      ['(不明)', '1', '2026-08-10T09:00:00Z'],
    ]);
    expect(rows(html, 'usage-cross')).toEqual([
      ['hanako@f-a-c.co.jp', 'タブを開く: 媒体分析', '42', '2026-08-10T09:00:00Z'],
      ['hanako@f-a-c.co.jp', 'CSV取込', '7', '2026-08-10T09:00:00Z'],
      ['(不明)', 'mystery_event', '1', '2026-08-10T09:00:00Z'],
    ]);
    expect(html).not.toContain('upload_survey_csv');
    expect(html).toContain('日時は協定世界時(UTC)です。ログは1年で自動削除されます。');
  });

  it('explains an empty period instead of showing a blank table', () => {
    const html = renderToStaticMarkup(<UsageView data={usage7Empty} />);
    expect(usage7Empty.days).toBe(7);
    expect(html).toContain('class="w8-pill w8-pill-active">直近7日</a>');
    expect(rows(html, 'usage-by-event')).toEqual([['この期間の記録はまだありません。']]);
    expect(rows(html, 'usage-cross')).toEqual([['この期間の記録はまだありません。']]);
  });
});

describe('route', () => {
  it('parses the query string into the four views with defaults', () => {
    expect(parseAdminRoute('')).toEqual({ view: 'usage', days: 30 });
    expect(parseAdminRoute('?view=usage&days=7')).toEqual({ view: 'usage', days: 7 });
    expect(parseAdminRoute('?view=usage&days=abc')).toEqual({ view: 'usage', days: 30 });
    expect(parseAdminRoute('?view=users')).toEqual({ view: 'users' });
    expect(parseAdminRoute('?view=user&id=acc-0001')).toEqual({ view: 'user', id: 'acc-0001' });
    expect(parseAdminRoute('?view=user')).toEqual({ view: 'users' });
    expect(parseAdminRoute('?view=login-failures')).toEqual({ view: 'login-failures' });
    expect(parseAdminRoute('?view=hubspot')).toEqual({ view: 'hubspot' });
  });

  it('maps views to hrefs and admin JSON endpoints', () => {
    expect(adminHref({ view: 'user', id: 'a b' })).toBe('?view=user&id=a+b');
    expect(adminHref({ view: 'usage', days: 90 })).toBe('?view=usage&days=90');
    expect(adminApiPath({ view: 'usage', days: 90 })).toBe('/api/admin/usage?days=90');
    expect(adminApiPath({ view: 'users' })).toBe('/api/admin/users');
    expect(adminApiPath({ view: 'user', id: 'a/b' })).toBe('/api/admin/users/a%2Fb');
    expect(adminApiPath({ view: 'login-failures' })).toBe('/api/admin/login-failures');
    expect(adminApiPath({ view: 'hubspot' })).toBe('/api/admin/hubspot-usage');
    expect(adminHref({ view: 'hubspot' })).toBe('?view=hubspot');
  });
});

describe('errors', () => {
  it('says 管理者のみ on 403 and login on auth errors', () => {
    expect(describeApiError(new ApiHttpError(403)).title).toBe('管理者のみ');
    expect(describeApiError(new AuthRequiredError('x')).title).toBe('ログインが必要です');
    expect(describeApiError(new ApiHttpError(500))).toEqual({
      title: 'サーバ応答を取得できませんでした',
      detail: 'HTTP 500',
    });
    const nf = { title: 'アカウントが見つかりません', detail: 'ID: acc-x' };
    expect(describeApiError(new ApiHttpError(404), nf)).toBe(nf);
    const html = renderToStaticMarkup(<ErrorBox title="管理者のみ" detail="d" />);
    expect(html).toContain('<h1>管理者のみ</h1>');
  });

  it('renders the same nav links as the old admin pages', () => {
    const html = renderToStaticMarkup(<AdminNav />);
    expect(html).toContain('href="/"');
    expect(html).toContain('href="?view=usage&amp;days=30">利用状況</a>');
    expect(html).toContain('href="?view=users">ユーザー一覧</a>');
    expect(html).toContain('href="?view=login-failures">失敗監視</a>');
    expect(html).toContain('href="?view=hubspot">HubSpot</a>');
    expect(html).toContain('href="/app/my?view=activity"');
  });
});
