import { useState, type SyntheticEvent } from 'react';
import { accounts, customers } from '../../fixtures';
import { usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { IconCheck } from '../../ui/icons';
import { Button, Field, Notice } from '../../ui/kit';
import { BTN_LG, CheckRow, Fold, Heading, NETWORK_ERROR, PasswordInput, useFakeSend } from './parts';

const INVITED = accounts.find((a) => a.status === 'invited');
const EMAIL = INVITED?.email ?? '';
const CUSTOMER = customers.find((c) => c.id === INVITED?.customerId)?.name ?? '';

function Check({ ok, children }: { ok: boolean; children: string }) {
  return (
    <li className={ok ? 'text-pt-ok' : 'text-pt-muted'}>
      <span aria-hidden="true" className="mr-1.5 inline-flex w-4 justify-center align-middle">{ok ? <IconCheck size={13} /> : '・'}</span>
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
  const [remember, setRemember] = useState(false);
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
    <AuthLayout company={CUSTOMER}>
      <Heading title="パスワードの設定" lead="パスワードを決めてください。" />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {offline && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
        <div>
          <p className="text-sm font-medium text-pt-ink">メールアドレス</p>
          <p className="mt-1 break-all text-[13px] text-pt-ink">{EMAIL}</p>
        </div>
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
        <CheckRow checked={remember} onChange={setRemember} label="この端末を覚える" note="90 日。自分だけが使う端末のとき" />
        <Button type="submit" variant="primary" className={BTN_LG} disabled={sending}>{sending ? '設定しています…' : '設定して始める'}</Button>
      </form>
    </AuthLayout>
  );
}
