// /app/admin (W8): React version of the four Rust admin pages.
// Data comes from GET /api/admin/* (require_admin in Rust; 403 for non-admins).
// The views are pure functions of the JSON so they can be tested with fixtures.
import { createContext, useContext, type MouseEvent, type ReactNode } from 'react';
import type { AdminLoginFailuresResponse } from '../../generated/AdminLoginFailuresResponse';
import type { AdminUsageEntry } from '../../generated/AdminUsageEntry';
import type { AdminUsageResponse } from '../../generated/AdminUsageResponse';
import type { AdminUserDetailResponse } from '../../generated/AdminUserDetailResponse';
import type { AdminUsersResponse } from '../../generated/AdminUsersResponse';
import { dashIfEmpty, truncateChars, unknownIfEmpty } from './format';
import { adminApiPath, adminHref, parseAdminRoute, type AdminRoute } from './route';
import { describeApiError, useApiGet } from './useApiGet';
import { useQueryRoute } from './useQueryRoute';

/** Rust: `s.user_agent.chars().take(40)` in admin/render.rs. */
export const ADMIN_UA_CHARS = 40;

const NavigateContext = createContext<(route: AdminRoute) => void>(() => undefined);

/** In-screen link: renders a real href (works without JS / in tests) and pushes history on click. */
export function AdminLink({
  route,
  className,
  children,
}: {
  route: AdminRoute;
  className?: string;
  children: ReactNode;
}) {
  const navigate = useContext(NavigateContext);
  const onClick = (e: MouseEvent<HTMLAnchorElement>): void => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    navigate(route);
  };
  return (
    <a href={adminHref(route)} className={className} onClick={onClick}>
      {children}
    </a>
  );
}

export function AdminNav() {
  return (
    <nav className="w8-nav" aria-label="管理">
      <a href="/" className="w8-nav-muted">
        ← ダッシュボード
      </a>
      <span className="w8-nav-sep">|</span>
      <AdminLink route={{ view: 'usage', days: 30 }}>利用状況</AdminLink>
      <AdminLink route={{ view: 'users' }}>ユーザー一覧</AdminLink>
      <AdminLink route={{ view: 'login-failures' }}>失敗監視</AdminLink>
      <a href="/app/my?view=activity" className="w8-nav-right">
        自分の履歴
      </a>
    </nav>
  );
}

export function ErrorBox({ title, detail, red }: { title: string; detail: string; red?: boolean }) {
  return (
    <div role="alert" className={red === true ? 'w8-alert w8-alert-red' : 'w8-alert'} data-testid="admin-error">
      <h1>{title}</h1>
      <p>{detail}</p>
    </div>
  );
}

// ---------------------------------------------------------------- users list

export function UsersListView({ data }: { data: AdminUsersResponse }) {
  return (
    <>
      <h1 className="w8-h1">ユーザー一覧 ({data.accounts.length} 件)</h1>
      <div className="w8-table-wrap">
        <table className="w8-table" data-testid="users-table">
          <thead>
            <tr>
              <th>メール</th>
              <th>氏名</th>
              <th>会社</th>
              <th>権限</th>
              <th className="w8-right">ログイン回数</th>
              <th>最終ログイン</th>
              <th>初回ログイン</th>
              <th>状態</th>
            </tr>
          </thead>
          <tbody>
            {data.accounts.map((a) => (
              <tr key={a.id}>
                <td>
                  <AdminLink route={{ view: 'user', id: a.id }} className="w8-link">
                    {a.email}
                  </AdminLink>
                </td>
                <td>{dashIfEmpty(a.display_name)}</td>
                <td>{dashIfEmpty(a.company)}</td>
                <td>
                  <span className={a.role === 'admin' ? 'w8-badge w8-badge-admin' : 'w8-muted w8-xs'}>
                    {a.role}
                  </span>
                </td>
                <td className="w8-right">{a.login_count}</td>
                <td className="w8-muted w8-xs">{a.last_login_at}</td>
                <td className="w8-muted w8-xs">{a.first_seen_at}</td>
                <td>{a.disabled_at === '' ? '' : <span className="w8-badge w8-badge-disabled">無効</span>}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- user detail

function SuccessCell({ success }: { success: number }) {
  return success === 1 ? <span className="w8-green">成功</span> : <span className="w8-red">失敗</span>;
}

export function UserDetailView({ data }: { data: AdminUserDetailResponse }) {
  const acc = data.account;
  const k = data.kpi_30d;
  return (
    <>
      <section className="w8-card">
        <h2 className="w8-h2" data-testid="detail-email">
          {acc.email}
        </h2>
        <div className="w8-grid-4" data-testid="detail-profile">
          <div>
            <span className="w8-label">氏名</span>
            {dashIfEmpty(acc.display_name)}
          </div>
          <div>
            <span className="w8-label">会社</span>
            {dashIfEmpty(acc.company)}
          </div>
          <div>
            <span className="w8-label">権限</span>
            {acc.role}
          </div>
          <div>
            <span className="w8-label">ログイン回数</span>
            {acc.login_count}
          </div>
          <div>
            <span className="w8-label">初回</span>
            {acc.first_seen_at}
          </div>
          <div>
            <span className="w8-label">最終</span>
            {acc.last_login_at}
          </div>
          <div>
            <span className="w8-label">ID</span>
            <code className="w8-code">{acc.id}</code>
          </div>
          <div>
            <span className="w8-label">状態</span>
            {acc.disabled_at === '' ? <span className="w8-green">有効</span> : <span className="w8-red">無効</span>}
          </div>
        </div>
      </section>

      <div className="w8-kpis" data-testid="detail-kpis">
        <div className="w8-kpi">
          <div className="w8-kpi-title">直近30日 ログイン成功</div>
          <div className="w8-kpi-value" data-testid="kpi-login-ok">
            {k.login_ok}
          </div>
        </div>
        <div className="w8-kpi">
          <div className="w8-kpi-title">直近30日 ログイン失敗</div>
          <div className="w8-kpi-value w8-red" data-testid="kpi-login-fail">
            {k.login_fail}
          </div>
        </div>
        <div className="w8-kpi">
          <div className="w8-kpi-title">直近30日 操作数</div>
          <div className="w8-kpi-value" data-testid="kpi-activity">
            {k.activity}
          </div>
        </div>
        <div className="w8-kpi">
          <div className="w8-kpi-title">直近30日 企業閲覧数</div>
          <div className="w8-kpi-value" data-testid="kpi-company-views">
            {k.company_views}
          </div>
        </div>
      </div>

      <section style={{ marginBottom: 24 }}>
        <h3 className="w8-h3">ログイン履歴 ({data.sessions.length} 件)</h3>
        <div className="w8-table-wrap">
          <table className="w8-table w8-table-dense" data-testid="sessions-table">
            <thead>
              <tr>
                <th>日時</th>
                <th>結果</th>
                <th>方式</th>
                <th>IPハッシュ</th>
                <th>User-Agent</th>
                <th>失敗理由</th>
              </tr>
            </thead>
            <tbody>
              {data.sessions.map((s) => (
                <tr key={s.id}>
                  <td className="w8-xs">{s.started_at}</td>
                  <td>
                    <SuccessCell success={s.success} />
                  </td>
                  <td className="w8-xs">{s.login_method}</td>
                  <td className="w8-xs w8-dim">{s.ip_hash}</td>
                  <td className="w8-xs w8-dim">{truncateChars(s.user_agent, ADMIN_UA_CHARS)}</td>
                  <td className="w8-xs w8-red">{s.failure_reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
      <section>
        <h3 className="w8-h3">操作履歴 ({data.activities.length} 件)</h3>
        <div className="w8-table-wrap">
          <table className="w8-table w8-table-dense" data-testid="activities-table">
            <thead>
              <tr>
                <th>日時</th>
                <th>イベント</th>
                <th>対象種別</th>
                <th>対象ID</th>
              </tr>
            </thead>
            <tbody>
              {data.activities.map((a) => (
                <tr key={a.id}>
                  <td className="w8-xs">{a.at}</td>
                  <td>{a.event_type}</td>
                  <td className="w8-xs w8-muted">{a.target_type}</td>
                  <td className="w8-xs">{a.target_id}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}

// ---------------------------------------------------------------- login failures

export function LoginFailuresView({ data }: { data: AdminLoginFailuresResponse }) {
  return (
    <>
      <h1 className="w8-h1">ログイン失敗ログ ({data.failures.length} 件)</h1>
      <p className="w8-subtle">
        直近の失敗のみを表示。同一 ip_hash の連続失敗は不正アクセスの可能性があるため確認してください。
      </p>
      <div className="w8-table-wrap">
        <table className="w8-table w8-table-dense" data-testid="failures-table">
          <thead>
            <tr>
              <th>日時</th>
              <th>試行メール</th>
              <th>失敗理由</th>
              <th>IPハッシュ</th>
              <th>User-Agent</th>
            </tr>
          </thead>
          <tbody>
            {data.failures.map((f) => (
              <tr key={f.id}>
                <td className="w8-xs">{f.started_at}</td>
                <td>{f.attempted_email}</td>
                <td className="w8-xs w8-red">{f.failure_reason}</td>
                <td className="w8-xs w8-dim">{f.ip_hash}</td>
                <td className="w8-xs w8-dim">{truncateChars(f.user_agent, ADMIN_UA_CHARS)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- usage

const USAGE_PERIODS = [7, 30, 90] as const;
const USAGE_EMPTY = 'この期間の記録はまだありません。';

function UsageTable({
  testId,
  head,
  rows,
  colSpan,
  children,
}: {
  testId: string;
  head: ReactNode;
  rows: AdminUsageEntry[];
  colSpan: number;
  children: (row: AdminUsageEntry) => ReactNode;
}) {
  return (
    <div className="w8-table-wrap">
      <table className="w8-table" data-testid={testId}>
        <thead>
          <tr>{head}</tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={colSpan} className="w8-empty">
                {USAGE_EMPTY}
              </td>
            </tr>
          ) : (
            rows.map((r, i) => (
              <tr key={`${r.account_id}|${r.event_type}|${r.target_id}|${String(i)}`}>{children(r)}</tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

export function UsageView({ data }: { data: AdminUsageResponse }) {
  return (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 4 }}>
        利用状況
      </h1>
      <p className="w8-subtle">
        誰が・どの機能を・どれだけ使ったかの集計です。記録しているのはタブ切替・検索実行・レポート生成・CSV取込などの操作で、入力途中の絞り込みや画面の再描画は含みません。
      </p>
      <div className="w8-pills" data-testid="usage-periods">
        {USAGE_PERIODS.map((d) => (
          <AdminLink
            key={d}
            route={{ view: 'usage', days: d }}
            className={d === data.days ? 'w8-pill w8-pill-active' : 'w8-pill'}
          >
            直近{d}日
          </AdminLink>
        ))}
      </div>

      <div className="w8-grid-2">
        <div>
          <h2 className="w8-section-title">機能別</h2>
          <UsageTable
            testId="usage-by-event"
            colSpan={3}
            rows={data.by_event}
            head={
              <>
                <th>機能</th>
                <th className="w8-right">回数</th>
                <th>最終利用</th>
              </>
            }
          >
            {(r) => (
              <>
                <td>{r.label}</td>
                <td className="w8-right w8-emerald">{r.count}</td>
                <td className="w8-muted w8-xs">{r.last_at}</td>
              </>
            )}
          </UsageTable>
        </div>
        <div>
          <h2 className="w8-section-title">ユーザー別</h2>
          <UsageTable
            testId="usage-by-account"
            colSpan={3}
            rows={data.by_account}
            head={
              <>
                <th>ユーザー</th>
                <th className="w8-right">操作回数</th>
                <th>最終利用</th>
              </>
            }
          >
            {(r) => (
              <>
                <td>
                  <AdminLink route={{ view: 'user', id: r.account_id }} className="w8-link">
                    {unknownIfEmpty(r.email)}
                  </AdminLink>
                </td>
                <td className="w8-right w8-emerald">{r.count}</td>
                <td className="w8-muted w8-xs">{r.last_at}</td>
              </>
            )}
          </UsageTable>
        </div>
      </div>

      <h2 className="w8-section-title" style={{ marginTop: 24 }}>
        ユーザー × 機能（上位100件）
      </h2>
      <UsageTable
        testId="usage-cross"
        colSpan={4}
        rows={data.cross}
        head={
          <>
            <th>ユーザー</th>
            <th>機能</th>
            <th className="w8-right">回数</th>
            <th>最終利用</th>
          </>
        }
      >
        {(r) => (
          <>
            <td>{unknownIfEmpty(r.email)}</td>
            <td>{r.label}</td>
            <td className="w8-right w8-emerald">{r.count}</td>
            <td className="w8-muted w8-xs">{r.last_at}</td>
          </>
        )}
      </UsageTable>
      <p className="w8-note">日時は協定世界時(UTC)です。ログは1年で自動削除されます。</p>
    </>
  );
}

// ---------------------------------------------------------------- screen

function AdminBody({ route }: { route: AdminRoute }) {
  const state = useApiGet<
    AdminUsersResponse | AdminUserDetailResponse | AdminLoginFailuresResponse | AdminUsageResponse
  >(adminApiPath(route));
  if (state.status === 'loading') return <p data-testid="admin-loading">読み込み中…</p>;
  if (state.status === 'error') {
    const notFound =
      route.view === 'user'
        ? { title: 'アカウントが見つかりません', detail: `ID: ${route.id}` }
        : undefined;
    const text = describeApiError(state.error, notFound);
    return <ErrorBox title={text.title} detail={text.detail} />;
  }
  switch (route.view) {
    case 'users':
      return <UsersListView data={state.data as AdminUsersResponse} />;
    case 'user':
      return <UserDetailView data={state.data as AdminUserDetailResponse} />;
    case 'login-failures':
      return <LoginFailuresView data={state.data as AdminLoginFailuresResponse} />;
    case 'usage':
      return <UsageView data={state.data as AdminUsageResponse} />;
  }
}

export function AdminScreen() {
  const [route, navigate] = useQueryRoute(parseAdminRoute, adminHref);
  return (
    <NavigateContext.Provider value={navigate}>
      <div className="w8-page">
        <AdminNav />
        <AdminBody route={route} />
      </div>
    </NavigateContext.Provider>
  );
}
