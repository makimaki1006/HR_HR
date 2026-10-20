import { useEffect, useState, type SyntheticEvent } from 'react';
import { LOCK_MINUTES, SUPPORT } from '../../fixtures';
import { log } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { IconMail, IconPhone } from '../../ui/icons';
import { Button, Field, Notice, TextInput } from '../../ui/kit';
import { BTN_LG, ContactLines, Heading, INPUT_LG, NETWORK_ERROR, PasswordInput, TOUCH, mailHref, useFakeSend } from './parts';

export function Locked() {
  const { go } = usePreviewNav();
  const [left, setLeft] = useState((LOCK_MINUTES - 1) * 60);
  useEffect(() => {
    if (left <= 0) return undefined;
    const t = window.setTimeout(() => { setLeft((s) => s - 1); }, 1000);
    return () => { window.clearTimeout(t); };
  }, [left]);
  const minutes = Math.ceil(left / 60);
  const tel = `tel:${SUPPORT.tel.replace(/-/g, '')}`;
  return (
    <AuthLayout hideFooter>
      <Heading title="ログインを一時的に止めています" />
      {left > 0 ? (
        <p className="mb-5 text-[15px] text-pt-ink">あと <span className="pt-num text-2xl font-semibold">{minutes}</span> 分で解除されます</p>
      ) : (
        <p className="mb-5 text-[15px] text-pt-ink">ログインできるようになりました</p>
      )}
      {left > 0 && (
        <div className="mb-5">
          <p className="mb-2 text-[13px] text-pt-muted">お急ぎのときは弊社の担当者が解除します。</p>
          <div className="grid grid-cols-2 gap-2">
            <a href={tel} className="flex h-11 items-center justify-center gap-1.5 rounded-md border border-pt-line-strong bg-pt-surface text-[13px] font-medium text-pt-ink shadow-pt-card hover:bg-pt-bg"><IconPhone size={14} />電話する</a>
            <a href={mailHref('ログインできません')} className="flex h-11 items-center justify-center gap-1.5 rounded-md border border-pt-line-strong bg-pt-surface text-[13px] font-medium text-pt-ink shadow-pt-card hover:bg-pt-bg"><IconMail size={14} />メールで頼む</a>
          </div>
          <p className="mt-2 text-xs text-pt-faint"><span className="pt-num whitespace-nowrap">{SUPPORT.tel}</span>{' '}<span className="whitespace-nowrap">{SUPPORT.hours}</span></p>
        </div>
      )}
      <Button variant={left > 0 ? 'secondary' : 'primary'} className={BTN_LG} onClick={() => { go(ROUTES.login); }}>ログイン画面へ</Button>
      <p className="mt-4 text-center text-xs text-pt-faint">
        試作: <button type="button" className="underline" onClick={() => { setLeft(0); }}>待ち時間を飛ばす</button>
      </p>
    </AuthLayout>
  );
}

type ForgotStage = 'input' | 'sent' | 'reset' | 'done';

export function Forgot() {
  const { go } = usePreviewNav();
  const [stage, setStage] = useState<ForgotStage>('input');
  const [email, setEmail] = useState('');
  const [pw, setPw] = useState('');
  const [pw2, setPw2] = useState('');
  const [tried, setTried] = useState(false);
  const [offline, setOffline] = useState(false);
  const { sending, run } = useFakeSend();

  const validEmail = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim());
  const sendLink = (e: SyntheticEvent) => {
    e.preventDefault();
    setTried(true);
    if (!validEmail || sending) return;
    setOffline(false);
    run(() => {
      if (email.includes('offline')) setOffline(true);
      else setStage('sent');
    });
  };
  const same = pw !== '' && pw === pw2;
  const resetOk = pw.length >= 12 && same;
  const reset = (e: SyntheticEvent) => {
    e.preventDefault();
    setTried(true);
    if (!resetOk || sending) return;
    run(() => {
      log({ kind: 'save', detail: 'パスワードを再設定', accountEmail: email.trim() });
      setStage('done');
    });
  };

  return (
    <AuthLayout hideFooter>
      {stage === 'input' && (
        <>
          <Heading title="パスワードの再設定" lead="ご登録のメールアドレスに、再設定のリンクをお送りします。" />
          <form onSubmit={sendLink} className="flex flex-col gap-4" noValidate>
            {offline && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
            <Field label="メールアドレス" htmlFor="forgot-email" error={tried && !validEmail ? 'メールアドレスの形で入力してください' : undefined}>
              <TextInput id="forgot-email" name="email" type="email" inputMode="email" autoComplete="username" className={INPUT_LG} value={email} onChange={(e) => { setEmail(e.target.value); }} />
            </Field>
            <Button type="submit" variant="primary" className={BTN_LG} disabled={sending}>{sending ? '送っています…' : '再設定のリンクを送る'}</Button>
          </form>
        </>
      )}
      {stage === 'sent' && (
        <>
          <Heading title="メールを送りました" lead={<><span className="break-all text-pt-ink">{email.trim()}</span> に再設定のリンクを送りました。リンクは 1 時間、1 回だけ使えます。</>} />
          <p className="mb-5 text-xs text-pt-muted">届かないときは迷惑メールのフォルダもご確認ください。登録のないアドレスには送られません。</p>
          <Button variant="primary" className={BTN_LG} onClick={() => { setStage('reset'); setTried(false); }}>メールのリンクを開く（試作）</Button>
        </>
      )}
      {stage === 'reset' && (
        <>
          <Heading title="新しいパスワード" />
          <form onSubmit={reset} className="flex flex-col gap-4" noValidate>
            <Field label="パスワード" htmlFor="forgot-pw" hint="12 文字以上" error={tried && pw.length < 12 ? '12 文字以上で入力してください' : undefined}>
              <PasswordInput id="forgot-pw" name="new-password" value={pw} onChange={setPw} autoComplete="new-password" />
            </Field>
            <Field label="パスワード（確認）" htmlFor="forgot-pw2" error={(pw2 !== '' || tried) && !same ? 'パスワードが合っていません' : undefined}>
              <PasswordInput id="forgot-pw2" name="confirm-password" value={pw2} onChange={setPw2} autoComplete="new-password" invalid={(pw2 !== '' || tried) && !same} />
            </Field>
            <Button type="submit" variant="primary" className={BTN_LG} disabled={sending}>{sending ? '設定しています…' : '設定する'}</Button>
          </form>
        </>
      )}
      {stage === 'done' && (
        <>
          <Heading title="パスワードを変更しました" lead="新しいパスワードでログインしてください。" />
          <Button variant="primary" className={BTN_LG} onClick={() => { go(ROUTES.login); }}>ログイン画面へ</Button>
        </>
      )}
      {stage !== 'done' && (
        <div className="mt-5 border-t border-pt-line pt-4">
          <p className="mb-1.5 text-xs text-pt-faint">メールが使えないときは担当者へ</p>
          <ContactLines subject="パスワードの再設定" />
          <Button variant="ghost" size="sm" className={`${TOUCH} -ml-2.5 mt-2`} onClick={() => { go(ROUTES.login); }}>ログインに戻る</Button>
        </div>
      )}
    </AuthLayout>
  );
}

export function LoggedOut() {
  const { go } = usePreviewNav();
  const [forgotten, setForgotten] = useState(false);
  return (
    <AuthLayout>
      <Heading title="ログアウトしました" lead="この画面は閉じて構いません。" />
      <Button variant="primary" className={BTN_LG} onClick={() => { go(ROUTES.login); }}>ログイン画面へ戻る</Button>
      <div className="mt-4 border-t border-pt-line pt-3 text-center">
        {forgotten ? (
          <p className="text-xs text-pt-ok">この端末の記憶を消しました。次回は確認コードが必要です。</p>
        ) : (
          <Button
            variant="ghost"
            size="sm"
            className={TOUCH}
            onClick={() => { setForgotten(true); log({ kind: 'device_removed', detail: 'ログアウトの画面で、この端末の記憶を消した' }); }}
          >
            この端末の記憶も消す
          </Button>
        )}
      </div>
    </AuthLayout>
  );
}

export function InviteExpired() {
  return (
    <AuthLayout hideFooter>
      <Heading title="このリンクは使えません" lead="招待のリンクは、送ってから 1 週間で切れます。また、1 回使うと使えなくなります。" />
      <p className="mb-2 text-[13px] text-pt-muted">弊社の担当者に、招待をもう一度出すようお伝えください。</p>
      <ContactLines subject="招待の出し直し" />
    </AuthLayout>
  );
}
