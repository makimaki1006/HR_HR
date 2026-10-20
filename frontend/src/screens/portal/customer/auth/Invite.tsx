import { useState, type SyntheticEvent } from 'react';
import { accounts, customers } from '../../fixtures';
import { usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Field, Notice, TextInput } from '../../ui/kit';
import { Heading, NETWORK_ERROR, PasswordInput, useFakeSend } from './parts';

const INVITED = accounts.find((a) => a.status === 'invited');
const EMAIL = INVITED?.email ?? '';
const CUSTOMER = customers.find((c) => c.id === INVITED?.customerId)?.name ?? '';

function Check({ ok, children }: { ok: boolean; children: string }) {
  return (
    <li className={ok ? 'text-pt-ok' : 'text-pt-muted'}>
      <span aria-hidden="true" className="mr-1.5 inline-block w-4 text-center">{ok ? '✓' : '・'}</span>
      {children}
      <span className="sr-only">{ok ? '（満たしています）' : '（まだです）'}</span>
    </li>
  );
}

export function Invite() {
  const { go } = usePreviewNav();
  const [pw, setPw] = useState('');
  const [pw2, setPw2] = useState('');
  const [agree, setAgree] = useState(false);
  const [remember, setRemember] = useState(true);
  const [tried, setTried] = useState(false);
  const [offline, setOffline] = useState(false);
  const { sending, run } = useFakeSend();

  const longEnough = pw.length >= 12;
  const notEmail = pw !== '' && pw.toLowerCase() !== EMAIL;
  const same = pw !== '' && pw === pw2;
  const ready = longEnough && notEmail && same && agree;

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    setTried(true);
    if (!ready || sending) return;
    setOffline(false);
    run(() => {
      if (pw.includes('offline')) setOffline(true);
      else go('auth-welcome');
    });
  };

  return (
    <AuthLayout>
      <Heading title="パスワードの設定" lead={`${CUSTOMER} 様の応募者の管理にご招待しました。パスワードを決めてください。`} />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {offline && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
        <Field label="メールアドレス" htmlFor="inv-email" hint="招待を送ったメールアドレスです。変えられません。">
          <TextInput id="inv-email" value={EMAIL} readOnly disabled />
        </Field>
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
        <div className="text-sm">
          <details className="rounded-md border border-pt-line bg-pt-bg px-3 py-2 text-xs leading-relaxed text-pt-muted">
            <summary className="cursor-pointer text-pt-ink">利用規約を読む</summary>
            <ol className="mt-2 list-decimal space-y-1 pl-4">
              <li>この画面の応募者の情報は、貴社の採用の目的だけにお使いください。</li>
              <li>ログインの情報は、ほかの方に教えたり共有したりしないでください。</li>
              <li>画面で入力した内容は、弊社の担当者も見ることがあります。</li>
              <li>心当たりのないログインに気づいたときは、すぐに弊社へご連絡ください。</li>
            </ol>
          </details>
          <label className="mt-3 flex items-start gap-2">
            <input type="checkbox" checked={agree} className="mt-0.5 size-4" onChange={(e) => { setAgree(e.target.checked); }} />
            <span>利用規約に同意します</span>
          </label>
          {tried && !agree && <p className="mt-1 text-xs text-pt-danger">利用規約への同意が必要です</p>}
        </div>
        <label className="flex items-start gap-2 text-sm">
          <input type="checkbox" checked={remember} className="mt-0.5 size-4" onChange={(e) => { setRemember(e.target.checked); }} />
          <span>
            この端末を覚える（90 日）
            <span className="block text-xs text-pt-muted">招待のメールで本人を確かめているので、この端末では次回から確認コードが要りません。共用のパソコンでは外してください。</span>
          </span>
        </label>
        <Button type="submit" variant="primary" size="lg" disabled={sending}>{sending ? '設定しています…' : '設定して始める'}</Button>
      </form>
    </AuthLayout>
  );
}
