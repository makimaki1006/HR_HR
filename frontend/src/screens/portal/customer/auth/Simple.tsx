import { useEffect, useState } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Notice } from '../../ui/kit';
import { LOCK_MINUTES } from '../../fixtures';
import { ContactBox, Heading, TOUCH } from './parts';

export function Locked() {
  const { go } = usePreviewNav();
  const [left, setLeft] = useState((LOCK_MINUTES - 1) * 60);
  useEffect(() => {
    if (left <= 0) return undefined;
    const t = window.setTimeout(() => { setLeft((s) => s - 1); }, 1000);
    return () => { window.clearTimeout(t); };
  }, [left]);
  const minutes = Math.ceil(left / 60);
  return (
    <AuthLayout hideFooter>
      <Heading title="ログインを一時的に止めています" />
      <div className="mb-4">
        {left > 0 ? (
          <Notice tone="warn" title={`あと ${String(minutes)} 分ほどお待ちください`}>
            ログインに続けて失敗したため、安全のために一時的に入れなくしています。時間がたつと自動で解除されます。お急ぎのときは、弊社の担当者がすぐに解除できます。
          </Notice>
        ) : (
          <Notice tone="info" title="ログインできるようになりました">ログイン画面からもう一度お試しください。</Notice>
        )}
      </div>
      <p className="mb-2 text-sm text-pt-muted">お急ぎのときは、弊社の担当者にご連絡ください。</p>
      <ContactBox mailSubject="ログインできません" />
      <div className="mt-5 text-center">
        <Button variant={left > 0 ? 'ghost' : 'primary'} size={left > 0 ? 'sm' : 'lg'} className={left > 0 ? TOUCH : 'w-full'} onClick={() => { go(ROUTES.login); }}>ログイン画面へ</Button>
      </div>
      <p className="mt-4 text-center text-xs text-pt-faint">
        試作: <button type="button" className="underline" onClick={() => { setLeft(0); }}>待ち時間を飛ばす</button>
      </p>
    </AuthLayout>
  );
}

export function Forgot() {
  const { go } = usePreviewNav();
  return (
    <AuthLayout hideFooter>
      <Heading
        title="パスワードを忘れたとき"
        lead="弊社の担当者が、再設定のリンクをメールでお送りします。下の連絡先に、ご登録のメールアドレスをお知らせください。"
      />
      <ContactBox mailSubject="パスワードの再設定" />
      <p className="mt-4 text-xs leading-relaxed text-pt-muted">リンクは 1 週間有効で、1 回だけ使えます。届かないときは迷惑メールのフォルダもご確認ください。</p>
      <div className="mt-5">
        <Button className="h-11 w-full" onClick={() => { go(ROUTES.login); }}>ログインに戻る</Button>
      </div>
    </AuthLayout>
  );
}

export function LoggedOut() {
  const { go } = usePreviewNav();
  return (
    <AuthLayout>
      <Heading title="ログアウトしました" lead="ご利用ありがとうございました。この端末の画面は閉じて構いません。" />
      <Button variant="primary" size="lg" className="w-full" onClick={() => { go(ROUTES.login); }}>ログイン画面へ戻る</Button>
      <p className="mt-4 text-xs leading-relaxed text-pt-muted">この端末は覚えたままです。次回はコードなしでログインできます。共用のパソコンでは、設定の画面から覚えた端末を外せます。</p>
    </AuthLayout>
  );
}

export function InviteExpired() {
  return (
    <AuthLayout hideFooter>
      <Heading title="このリンクは使えません" />
      <div className="mb-4">
        <Notice tone="warn">招待のリンクは、送ってから 1 週間で切れます。また、1 回使うと使えなくなります。</Notice>
      </div>
      <p className="mb-2 text-sm text-pt-muted">弊社の担当者に、招待をもう一度出すようお伝えください。</p>
      <ContactBox mailSubject="招待の出し直し" />
    </AuthLayout>
  );
}
