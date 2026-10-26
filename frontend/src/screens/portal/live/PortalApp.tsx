// 顧客向け応募者画面の本番の入口 (/portal/*)。ログイン・確認コード・招待のパスワード設定と、ログイン後の入口。
// 見た目は試作 (customer/auth/*) の部品を使う。ログイン後は応募者の見る画面 (applicants/、計画の PR4)。
import { useCallback, useEffect, useState, type SyntheticEvent } from 'react';
import type { PortalInviteInfo } from '../../../generated/PortalInviteInfo';
import type { PortalLoginResponse } from '../../../generated/PortalLoginResponse';
import type { PortalMe } from '../../../generated/PortalMe';
import { CodeBoxes, toDigits } from '../customer/auth/CodeBoxes';
import { BTN_LG, CheckRow, Fold, Heading, INPUT_LG, NETWORK_ERROR, PasswordInput, TOUCH } from '../customer/auth/basics';
import { AuthLayout } from '../ui/layouts';
import { IconCheck } from '../ui/icons';
import { Button, Field, Notice, TextInput } from '../ui/kit';
import { call, inviteTokenFromHash, NETWORK, type ApiResult } from './api';
import { ApplicantsScreen } from './applicants/ApplicantsScreen';

const RESEND_WAIT = 60;
const MIN_PASSWORD = 12;

export const PATHS = { home: '/portal', login: '/portal/login', code: '/portal/code', invite: '/portal/invite' } as const;

interface NavState { to?: string[]; email?: string; notice?: string }
type Nav = (path: string, state?: NavState) => void;

function failMessage(r: Extract<ApiResult<unknown>, { ok: false }>): string {
  if (r.error === NETWORK) return NETWORK_ERROR;
  return r.message || 'うまくいきませんでした。少し待ってからもう一度お試しください';
}

/** ログインを送り、結果に合わせて次の画面へ。失敗なら文を返す */
async function signIn(email: string, password: string, nav: Nav): Promise<{ message: string; wrong: boolean } | null> {
  const r = await call<PortalLoginResponse>('POST', '/api/portal/login', { email, password });
  if (r.ok) {
    if (r.data.status === 'signed_in') nav(PATHS.home);
    else nav(PATHS.code, { to: r.data.to });
    return null;
  }
  return { message: failMessage(r), wrong: r.error === 'invalid_credentials' };
}

function LoginScreen({ nav, initialEmail, notice }: { nav: Nav; initialEmail?: string | undefined; notice?: string | undefined }) {
  const [email, setEmail] = useState(initialEmail ?? '');
  const [password, setPassword] = useState('');
  const [tried, setTried] = useState(false);
  const [sending, setSending] = useState(false);
  const [failures, setFailures] = useState(0);
  const [problem, setProblem] = useState<string | null>(null);

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    if (sending) return;
    setTried(true);
    if (email.trim() === '' || password === '') return;
    setSending(true);
    setProblem(null);
    void signIn(email.trim(), password, nav).then((fail) => {
      setSending(false);
      if (!fail) return;
      setProblem(fail.message);
      if (fail.wrong) {
        setFailures((n) => n + 1);
        setPassword('');
        setTried(false);
      }
    });
  };

  return (
    <AuthLayout>
      <Heading title="ログイン" />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {notice && !problem && <Notice tone="ok">{notice}</Notice>}
        {problem && (
          <Notice tone="danger">
            {problem}
            {failures >= 3 && <span className="mt-1 block">続けて間違えると、しばらくログインできなくなります。</span>}
          </Notice>
        )}
        <Field label="メールアドレス" htmlFor="login-email" error={tried && email.trim() === '' ? 'メールアドレスを入力してください' : undefined}>
          <TextInput id="login-email" name="email" type="email" inputMode="email" autoComplete="username" className={INPUT_LG} value={email} onChange={(e) => { setEmail(e.target.value); }} />
        </Field>
        <Field label="パスワード" htmlFor="login-password" error={tried && password === '' ? 'パスワードを入力してください' : undefined}>
          <PasswordInput id="login-password" name="password" value={password} onChange={setPassword} autoComplete="current-password" invalid={failures > 0 && problem !== null} />
        </Field>
        <Button type="submit" variant="primary" className={BTN_LG} disabled={sending}>{sending ? '確認しています…' : 'ログイン'}</Button>
      </form>
      <div className="mt-3">
        <Fold summary="パスワードを忘れたとき">
          <p>弊社の担当者にご連絡ください。パスワードを決め直すためのリンクをお送りします。</p>
        </Fold>
      </div>
    </AuthLayout>
  );
}

function CodeScreen({ nav, to }: { nav: Nav; to?: string[] | undefined }) {
  const [code, setCode] = useState('');
  const [remember, setRemember] = useState(true);
  const [problem, setProblem] = useState<{ tone: 'danger' | 'warn'; text: string } | null>(null);
  const [expired, setExpired] = useState(false);
  const [sending, setSending] = useState(false);
  const [wait, setWait] = useState(RESEND_WAIT);
  const [resent, setResent] = useState(false);
  const [recipients, setRecipients] = useState<string[]>(to ?? []);

  useEffect(() => {
    if (wait <= 0) return undefined;
    const t = window.setTimeout(() => { setWait((w) => w - 1); }, 1000);
    return () => { window.clearTimeout(t); };
  }, [wait]);

  const check = (c: string) => {
    if (c.length !== 6 || sending || expired) return;
    setSending(true);
    void call('POST', '/api/portal/login/code', { code: c, remember }).then((r) => {
      setSending(false);
      if (r.ok) { nav(PATHS.home); return; }
      if (r.error === 'wrong_code') {
        setProblem({ tone: 'danger', text: r.message });
        setCode('');
        window.setTimeout(() => { document.getElementById('code-0')?.focus(); }, 0);
      } else if (r.error === 'code_expired') {
        setExpired(true);
        setProblem({ tone: 'warn', text: r.message });
      } else {
        setProblem({ tone: 'warn', text: failMessage(r) });
      }
    });
  };

  const change = (raw: string) => {
    const c = toDigits(raw).slice(0, 6);
    setCode(c);
    setProblem(null);
    check(c);
  };

  const resend = () => {
    setProblem(null);
    void call<PortalLoginResponse>('POST', '/api/portal/login/resend').then((r) => {
      if (r.ok) {
        setWait(RESEND_WAIT);
        setResent(true);
        setCode('');
        setRecipients(r.data.to);
        return;
      }
      if (r.error === 'wait' && typeof r.extra.wait === 'number') setWait(r.extra.wait);
      if (r.error === 'code_expired' || r.error === 'resend_limit') setExpired(true);
      setProblem({ tone: 'warn', text: failMessage(r) });
    });
  };

  return (
    <AuthLayout>
      <Heading title="確認コード" lead="メールでお送りした 6 桁の数字を入力してください。" />
      <form onSubmit={(e) => { e.preventDefault(); check(code); }} className="flex flex-col gap-4" noValidate>
        {problem && <Notice tone={problem.tone}>{problem.text}</Notice>}
        <Field label="6 桁のコード">
          <CodeBoxes value={code} invalid={problem?.tone === 'danger'} onChange={change} />
        </Field>
        <CheckRow checked={remember} onChange={setRemember} label="この端末を覚える" note="90 日は確認コードを聞きません。ほかの人と共用する端末では外してください" />
        <Button type="submit" variant="primary" className={BTN_LG} disabled={code.length !== 6 || sending || expired}>{sending ? '確認しています…' : '確認する'}</Button>
      </form>
      <div className="mt-4 text-center text-sm">
        {resent && wait > 0 && <p className="mb-1 text-[13px] text-pt-ok">コードを送り直しました</p>}
        {!expired && (
          <Button variant="ghost" size="sm" className={TOUCH} disabled={wait > 0} onClick={resend}>
            {wait > 0 ? `コードを送り直す（あと ${String(wait)} 秒）` : 'コードを送り直す'}
          </Button>
        )}
        <div>
          <Button variant={expired ? 'primary' : 'ghost'} size="sm" className={TOUCH} onClick={() => { nav(PATHS.login); }}>ログインに戻る</Button>
        </div>
      </div>
      <div className="mt-2">
        <Fold summary="届かないとき">
          {recipients.length > 0 && <p>送り先: {recipients.join('、')}</p>}
          <p className="mt-1">コードの有効時間は 10 分です。迷惑メールのフォルダもご確認ください。届かないときは弊社の担当者にご連絡ください。</p>
        </Fold>
      </div>
    </AuthLayout>
  );
}

function Check({ ok, children }: { ok: boolean; children: string }) {
  return (
    <li className={ok ? 'text-pt-ok' : 'text-pt-muted'}>
      <span aria-hidden="true" className="mr-1.5 inline-flex w-4 justify-center align-middle">{ok ? <IconCheck size={13} /> : '・'}</span>
      {children}
      <span className="sr-only">{ok ? '（満たしています）' : '（まだです）'}</span>
    </li>
  );
}

function InviteScreen({ nav }: { nav: Nav }) {
  const [token] = useState(() => inviteTokenFromHash(window.location.hash));
  const [info, setInfo] = useState<PortalInviteInfo | null>(null);
  const [loadProblem, setLoadProblem] = useState<string | null>(() => (token ? null : 'このリンクは使えません。弊社の担当者にご連絡ください'));
  const [pw, setPw] = useState('');
  const [pw2, setPw2] = useState('');
  const [agree, setAgree] = useState(false);
  const [tried, setTried] = useState(false);
  const [sending, setSending] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    // 値をアドレス欄・履歴に残さない
    if (window.location.hash) window.history.replaceState(window.history.state, '', window.location.pathname);
    if (!token) return;
    void call<PortalInviteInfo>('POST', '/api/portal/invite/check', { token }).then((r) => {
      if (r.ok) setInfo(r.data);
      else setLoadProblem(failMessage(r));
    });
  }, [token]);

  const reset = info?.purpose === 'reset';
  const email = info?.email ?? '';
  const longEnough = Array.from(pw).length >= MIN_PASSWORD;
  const notEmail = pw !== '' && pw.trim().toLowerCase() !== email;
  const same = pw !== '' && pw === pw2;
  const ready = longEnough && notEmail && same && (reset || agree);

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    setTried(true);
    if (!ready || sending || !info) return;
    setSending(true);
    setProblem(null);
    void call('POST', '/api/portal/invite/accept', { token, password: pw }).then(async (r) => {
      if (!r.ok) { setSending(false); setProblem(failMessage(r)); return; }
      // 決めたパスワードでそのままログインする (新しい端末なので確認コードに進む)
      const fail = await signIn(email, pw, nav);
      setSending(false);
      if (fail) nav(PATHS.login, { email, notice: 'パスワードを設定しました。ログインしてください' });
    });
  };

  if (loadProblem) {
    return (
      <AuthLayout>
        <Heading title={'パスワードの設定'} />
        <Notice tone="warn">{loadProblem}</Notice>
        <div className="mt-4"><Button variant="ghost" size="sm" className={TOUCH} onClick={() => { nav(PATHS.login); }}>ログインの画面へ</Button></div>
      </AuthLayout>
    );
  }
  if (!info) {
    return <AuthLayout><p className="text-[13px] text-pt-muted">確かめています…</p></AuthLayout>;
  }

  return (
    <AuthLayout>
      <Heading title={reset ? 'パスワードの再設定' : 'パスワードの設定'} lead={reset ? '新しいパスワードを決めてください。前のパスワードと、覚えた端末は使えなくなります。' : 'パスワードを決めてください。'} />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {problem && <Notice tone="warn">{problem}</Notice>}
        <div>
          <p className="text-sm font-medium text-pt-ink">メールアドレス</p>
          <p className="mt-1 break-all text-[13px] text-pt-ink">{email}</p>
        </div>
        {/* パスワード管理の保存先にメールアドレスを結び付ける */}
        <input type="email" name="username" autoComplete="username" value={email} readOnly hidden />
        <Field label="パスワード" htmlFor="inv-pw" error={tried && !(longEnough && notEmail) ? 'パスワードの条件を満たしてください' : undefined}>
          <PasswordInput id="inv-pw" name="new-password" value={pw} onChange={setPw} autoComplete="new-password" />
        </Field>
        <ul className="-mt-2 space-y-0.5 text-xs">
          <Check ok={longEnough}>12 文字以上</Check>
          <Check ok={notEmail}>メールアドレスと同じではない</Check>
        </ul>
        <Field label="パスワード（確認）" htmlFor="inv-pw2" error={(pw2 !== '' || tried) && !same ? 'パスワードが合っていません' : undefined}>
          <PasswordInput id="inv-pw2" name="confirm-password" value={pw2} onChange={setPw2} autoComplete="new-password" invalid={(pw2 !== '' || tried) && !same} />
        </Field>
        {!reset && (
          <>
            <Fold summary="利用規約を読む">
              <ol className="list-decimal space-y-1 pl-4">
                <li>この画面の応募者の情報は、貴社の採用の目的だけにお使いください。</li>
                <li>ログインの情報は、ほかの方に教えたり共有したりしないでください。</li>
                <li>画面で入力した内容は、弊社の担当者も見ることがあります。</li>
                <li>心当たりのないログインに気づいたときは、すぐに弊社へご連絡ください。</li>
              </ol>
            </Fold>
            <div>
              <CheckRow checked={agree} onChange={setAgree} label="利用規約に同意します" />
              {tried && !agree && <p className="mt-1 text-xs text-pt-danger">利用規約への同意が必要です</p>}
            </div>
          </>
        )}
        <Button type="submit" variant="primary" className={BTN_LG} disabled={sending}>{sending ? '設定しています…' : '設定して始める'}</Button>
      </form>
    </AuthLayout>
  );
}

function HomeScreen({ nav }: { nav: Nav }) {
  const [me, setMe] = useState<PortalMe | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    void call<PortalMe>('GET', '/api/portal/me').then((r) => {
      if (r.ok) setMe(r.data);
      else if (r.status === 401) nav(PATHS.login);
      else setProblem(failMessage(r));
    });
  }, [nav]);

  const logout = () => {
    void call('POST', '/api/portal/logout').then(() => { nav(PATHS.login, { notice: 'ログアウトしました' }); });
  };

  const signedOut = useCallback(() => { nav(PATHS.login); }, [nav]);

  if (problem) return <AuthLayout><Notice tone="warn">{problem}</Notice></AuthLayout>;
  if (!me) return <AuthLayout><p className="text-[13px] text-pt-muted">確かめています…</p></AuthLayout>;
  return <ApplicantsScreen me={me} onSignedOut={signedOut} onLogout={logout} />;
}

interface Route extends NavState { path: string }

export function PortalApp() {
  const [route, setRoute] = useState<Route>(() => ({ path: window.location.pathname.replace(/\/+$/, '') || PATHS.home }));

  useEffect(() => {
    const onPop = () => { setRoute({ path: window.location.pathname.replace(/\/+$/, '') || PATHS.home }); };
    window.addEventListener('popstate', onPop);
    return () => { window.removeEventListener('popstate', onPop); };
  }, []);

  const nav = useCallback<Nav>((path, state) => {
    window.history.pushState(null, '', path);
    setRoute({ path, ...state });
  }, []);

  switch (route.path) {
    case PATHS.login: return <LoginScreen key={route.notice ?? 'login'} nav={nav} initialEmail={route.email} notice={route.notice} />;
    case PATHS.code: return <CodeScreen nav={nav} to={route.to} />;
    case PATHS.invite: return <InviteScreen nav={nav} />;
    default: return <HomeScreen nav={nav} />;
  }
}
