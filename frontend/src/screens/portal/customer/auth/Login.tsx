import { useState, type SyntheticEvent } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Field, Notice, TextInput } from '../../ui/kit';
import { Heading, NETWORK_ERROR, PasswordInput, TOUCH, useFakeSend } from './parts';

export function Login() {
  const { go } = usePreviewNav();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [failures, setFailures] = useState(0);
  const [offline, setOffline] = useState(false);
  const [tried, setTried] = useState(false);
  const { sending, run } = useFakeSend();

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    if (sending) return;
    setTried(true);
    if (email.trim() === '' || password === '') return;
    setOffline(false);
    run(() => {
      if (password === 'offline') {
        setOffline(true);
      } else if (password === 'wrong') {
        setFailures((n) => n + 1);
      } else {
        setFailures(0);
        if (password === 'locked') go('auth-locked');
        else go(password === 'known' ? ROUTES.list : 'auth-code');
      }
    });
  };

  return (
    <AuthLayout>
      <Heading title="ログイン" />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {failures > 0 && (
          <Notice tone="danger">
            メールアドレスかパスワードが違います
            {failures >= 3 && <span className="mt-1 block">続けて間違えると、15 分ほどログインできなくなります。</span>}
          </Notice>
        )}
        {offline && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
        <Field label="メールアドレス" htmlFor="login-email" error={tried && email.trim() === '' ? 'メールアドレスを入力してください' : undefined}>
          <TextInput id="login-email" name="email" type="email" inputMode="email" autoComplete="username" value={email} onChange={(e) => { setEmail(e.target.value); }} />
        </Field>
        <Field label="パスワード" htmlFor="login-password" error={tried && password === '' ? 'パスワードを入力してください' : undefined}>
          <PasswordInput id="login-password" name="password" value={password} onChange={setPassword} autoComplete="current-password" invalid={failures > 0} />
        </Field>
        <Button type="submit" variant="primary" size="lg" disabled={sending}>{sending ? '確認しています…' : 'ログイン'}</Button>
      </form>
      <div className="mt-5 text-center">
        <Button variant="ghost" size="sm" className={TOUCH} onClick={() => { go('auth-forgot'); }}>パスワードを忘れたとき</Button>
      </div>
      <details className="mt-4 border-t border-pt-line pt-3 text-xs text-pt-faint">
        <summary className="cursor-pointer">試作の操作</summary>
        <p className="mt-1 leading-relaxed">パスワード known: 覚えた端末で一覧へ／wrong: 誤り／locked: ロック／offline: 通信できない／それ以外: 確認コードへ</p>
      </details>
    </AuthLayout>
  );
}
