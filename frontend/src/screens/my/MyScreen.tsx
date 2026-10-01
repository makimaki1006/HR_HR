// /app/my (W8): React version of /my/profile and /my/activity.
// Reads GET /api/my/{profile,activity}; the profile form posts JSON to
// POST /api/my/profile, which runs the same Rust write path as the old HTML form.
import { useState, type MouseEvent, type ReactNode, type SubmitEvent } from 'react';
import type { AccountRow } from '../../generated/AccountRow';
import type { MyActivityResponse } from '../../generated/MyActivityResponse';
import type { MyProfileResponse } from '../../generated/MyProfileResponse';
import type { MyProfileUpdateRequest } from '../../generated/MyProfileUpdateRequest';
import { truncateChars } from '../admin/format';
import { describeApiError, useApiGet } from '../admin/useApiGet';
import { useQueryRoute } from '../admin/useQueryRoute';
import { postJson } from './postJson';
import { myApiPath, myHref, parseMyRoute, type MyRoute } from './route';

/** Rust: `s.user_agent.chars().take(50)` in my/render.rs. */
export const MY_UA_CHARS = 50;
export const PROFILE_POST_PATH = '/api/my/profile';
export const PROFILE_SAVED_MESSAGE = 'プロフィールを更新しました';
/** Rust: `chars().take(80)` / `take(120)` in my/handlers.rs; the inputs carry the same maxlength. */
export const DISPLAY_NAME_MAX = 80;
export const COMPANY_MAX = 120;

function MyNav({ navigate }: { navigate: (route: MyRoute) => void }) {
  const link = (route: MyRoute, label: string): ReactNode => {
    const onClick = (e: MouseEvent<HTMLAnchorElement>): void => {
      if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      e.preventDefault();
      navigate(route);
    };
    return (
      <a href={myHref(route)} onClick={onClick}>
        {label}
      </a>
    );
  };
  return (
    <nav className="w8-nav" aria-label="個人設定">
      <a href="/" className="w8-nav-muted">
        ← ダッシュボード
      </a>
      <span className="w8-nav-sep">|</span>
      {link({ view: 'profile' }, 'プロフィール')}
      {link({ view: 'activity' }, '自分の履歴')}
      <a href="/logout" className="w8-nav-muted w8-nav-right">
        ログアウト
      </a>
    </nav>
  );
}

/** Same wording as my/render.rs audit_disabled_page / not_linked_page. */
export function MyStatusBox({ status }: { status: 'audit_disabled' | 'not_linked' }) {
  if (status === 'audit_disabled') {
    return (
      <div role="alert" className="w8-alert" data-testid="my-status">
        <h1>この機能は現在ご利用いただけません</h1>
        <p>システム管理者が監査機能を有効化すると利用可能になります。</p>
      </div>
    );
  }
  return (
    <div role="alert" className="w8-alert" data-testid="my-status">
      <h1>アカウントが見つかりません</h1>
      <p>
        一度{' '}
        <a href="/logout" className="w8-link" style={{ textDecoration: 'underline' }}>
          ログアウト
        </a>{' '}
        して再ログインしてください。
      </p>
    </div>
  );
}

// ---------------------------------------------------------------- profile

export function ProfileForm({
  account,
  onSaved,
}: {
  account: AccountRow;
  onSaved: (account: AccountRow) => void;
}) {
  const [displayName, setDisplayName] = useState(account.display_name);
  const [company, setCompany] = useState(account.company);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const onSubmit = (e: SubmitEvent<HTMLFormElement>): void => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    const body: MyProfileUpdateRequest = { display_name: displayName, company };
    void postJson<MyProfileResponse>(PROFILE_POST_PATH, body).then((r) => {
      setBusy(false);
      if (!r.ok) {
        setError(describeApiError(r.error).title + ': ' + r.error.message);
        return;
      }
      if (r.data.status === 'ok') {
        onSaved(r.data.account);
      } else {
        setError(r.data.status === 'not_linked' ? 'アカウントが見つかりません' : 'この機能は現在ご利用いただけません');
      }
    });
  };

  return (
    <form className="w8-form" onSubmit={onSubmit} data-testid="profile-form">
      <label>
        <span>氏名</span>
        <input
          type="text"
          name="display_name"
          maxLength={DISPLAY_NAME_MAX}
          value={displayName}
          onChange={(e) => {
            setDisplayName(e.target.value);
          }}
        />
      </label>
      <label>
        <span>会社</span>
        <input
          type="text"
          name="company"
          maxLength={COMPANY_MAX}
          value={company}
          onChange={(e) => {
            setCompany(e.target.value);
          }}
        />
      </label>
      <button type="submit" className="w8-button" disabled={busy}>
        保存
      </button>
      {error !== null && (
        <p role="alert" className="w8-red" data-testid="profile-error">
          {error}
        </p>
      )}
    </form>
  );
}

export function ProfileView({
  account,
  flash,
  onSaved,
}: {
  account: AccountRow;
  flash: string | null;
  onSaved: (account: AccountRow) => void;
}) {
  return (
    <>
      {flash !== null && (
        <div className="w8-flash" data-testid="profile-flash">
          {flash}
        </div>
      )}
      <h1 className="w8-h1" style={{ marginBottom: 24 }}>
        プロフィール
      </h1>
      <div className="w8-grid-2">
        <section className="w8-card">
          <h2 className="w8-h3">基本情報（読み取り専用）</h2>
          <dl className="w8-dl" data-testid="profile-readonly">
            <dt>メール</dt>
            <dd>{account.email}</dd>
            <dt>権限</dt>
            <dd>{account.role}</dd>
            <dt>初回ログイン</dt>
            <dd className="w8-muted w8-xs">{account.first_seen_at}</dd>
            <dt>最終ログイン</dt>
            <dd className="w8-muted w8-xs">{account.last_login_at}</dd>
            <dt>ログイン回数</dt>
            <dd>{account.login_count}</dd>
          </dl>
        </section>
        <section className="w8-card">
          <h2 className="w8-h3">編集可能</h2>
          {/* keyed by the saved values so a re-fetched account re-seeds the inputs */}
          <ProfileForm
            key={`${account.id}|${account.display_name}|${account.company}`}
            account={account}
            onSaved={onSaved}
          />
        </section>
      </div>
    </>
  );
}

function ProfileBody() {
  const state = useApiGet<MyProfileResponse>(myApiPath({ view: 'profile' }));
  const [saved, setSaved] = useState<{ account: AccountRow; flash: string } | null>(null);
  if (state.status === 'loading') return <p data-testid="my-loading">読み込み中…</p>;
  if (state.status === 'error') {
    const t = describeApiError(state.error);
    return (
      <div role="alert" className="w8-alert" data-testid="my-error">
        <h1>{t.title}</h1>
        <p>{t.detail}</p>
      </div>
    );
  }
  if (state.data.status !== 'ok') return <MyStatusBox status={state.data.status} />;
  const onSaved = (account: AccountRow): void => {
    setSaved({ account, flash: PROFILE_SAVED_MESSAGE });
  };
  return saved !== null ? (
    <ProfileView account={saved.account} flash={saved.flash} onSaved={onSaved} />
  ) : (
    <ProfileView account={state.data.account} flash={null} onSaved={onSaved} />
  );
}

// ---------------------------------------------------------------- activity

export function ActivityView({ data }: { data: Extract<MyActivityResponse, { status: 'ok' }> }) {
  return (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 8 }} data-testid="activity-email">
        {data.account.email}
      </h1>
      <p className="w8-subtle" style={{ marginBottom: 24 }}>
        ご自身の最近の利用履歴 (直近50ログイン / 直近100操作)
      </p>
      <div className="w8-grid-2">
        <section className="w8-card">
          <h2 className="w8-h3">ログイン履歴</h2>
          <div className="w8-table-wrap">
            <table className="w8-table w8-table-dense" data-testid="my-sessions-table">
              <thead>
                <tr>
                  <th>日時</th>
                  <th>結果</th>
                  <th>端末</th>
                </tr>
              </thead>
              <tbody>
                {data.sessions.map((s) => (
                  <tr key={s.id}>
                    <td className="w8-xs">{s.started_at}</td>
                    <td>
                      {s.success === 1 ? <span className="w8-green">成功</span> : <span className="w8-red">失敗</span>}
                    </td>
                    <td className="w8-xs w8-dim">{truncateChars(s.user_agent, MY_UA_CHARS)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
        <section className="w8-card">
          <h2 className="w8-h3">操作履歴</h2>
          <div className="w8-table-wrap">
            <table className="w8-table w8-table-dense" data-testid="my-activities-table">
              <thead>
                <tr>
                  <th>日時</th>
                  <th>操作</th>
                  <th>種別</th>
                  <th>対象</th>
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
      </div>
    </>
  );
}

function ActivityBody() {
  const state = useApiGet<MyActivityResponse>(myApiPath({ view: 'activity' }));
  if (state.status === 'loading') return <p data-testid="my-loading">読み込み中…</p>;
  if (state.status === 'error') {
    const t = describeApiError(state.error);
    return (
      <div role="alert" className="w8-alert" data-testid="my-error">
        <h1>{t.title}</h1>
        <p>{t.detail}</p>
      </div>
    );
  }
  if (state.data.status !== 'ok') return <MyStatusBox status={state.data.status} />;
  return <ActivityView data={state.data} />;
}

export function MyScreen() {
  const [route, navigate] = useQueryRoute(parseMyRoute, myHref);
  return (
    <div className="w8-page">
      <MyNav navigate={navigate} />
      {route.view === 'activity' ? <ActivityBody /> : <ProfileBody />}
    </div>
  );
}
