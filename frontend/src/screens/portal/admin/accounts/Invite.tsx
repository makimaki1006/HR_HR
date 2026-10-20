import { useState } from 'react';
import { customers } from '../../fixtures';
import { usePreviewNav } from '../../preview';
import { Button, Card, Field, Notice, Select, TextArea, TextInput, formatShortDateTime } from '../../ui/kit';
import { customerName, findByEmail, invite, selectAccount, LOGIN_URL } from './store';
import { STATUS_LABEL, AccountsAdminLayout, CopyButton } from './shared';

interface Issued { link: string; expiresAt: string; email: string; customerId: string; displayName: string }

function mailText(i: Issued): string {
  const who = i.displayName ? i.displayName + ' 様' : 'ご担当者様';
  return [
    '件名: 応募者の管理画面のご案内',
    '',
    who,
    '',
    'いつもお世話になっております。',
    '応募者の状況を画面で確認できるようになりました。下のリンクから、パスワードを決めてご利用ください。',
    '',
    i.link,
    '',
    'リンクは 1 週間（' + formatShortDateTime(i.expiresAt) + ' まで）有効で、1 回だけ使えます。',
    '初めてお使いの端末では、登録のメールアドレスに確認コードが届きます。',
    '',
    '2 回目からは次の画面からログインしてください（ブックマークしておくと便利です）。',
    LOGIN_URL,
    'スマホでもお使いいただけます。',
    '',
    'ご不明な点があればお知らせください。',
  ].join('\n');
}

export function Invite() {
  const nav = usePreviewNav();
  const [customerId, setCustomerId] = useState(customers.find((c) => !c.disabled)?.id ?? '');
  const [email, setEmail] = useState('');
  const [name, setName] = useState('');
  const [issued, setIssued] = useState<Issued | null>(null);
  const [touched, setTouched] = useState(false);
  const formOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim());
  const existing = email.trim() !== '' ? findByEmail(email) : undefined;
  const formError = touched && email.trim() !== '' && !formOk ? 'メールアドレスの形になっていません。' : touched && email.trim() === '' ? 'メールアドレスを入力してください。' : null;

  const submit = () => {
    setTouched(true);
    if (!formOk || existing || customerId === '') return;
    const r = invite(customerId, email.trim(), name.trim());
    setIssued({ link: r.link, expiresAt: r.expiresAt, email: email.trim(), customerId, displayName: name.trim() });
  };
  const reset = () => { setIssued(null); setEmail(''); setName(''); setTouched(false); };
  const openExisting = (id: string) => { selectAccount(id); nav.go('admin-account-detail'); };

  return (
    <AccountsAdminLayout title="招待する">
      <div className="max-w-2xl space-y-4">
        {!issued ? (
          <Card className="space-y-4 p-5">
            <Field label="顧客" htmlFor="inv-c">
              <Select id="inv-c" value={customerId} onChange={(e) => { setCustomerId(e.target.value); }}>
                {customers.filter((c) => !c.disabled).map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </Select>
            </Field>
            <Field
              label="メールアドレス"
              htmlFor="inv-e"
              hint="ログインと確認コードの送り先になります。"
              error={
                existing ? (
                  <span>
                    このメールアドレスはすでに登録されています（{customerName(existing.customerId)}・{STATUS_LABEL[existing.status]}）。{' '}
                    <button type="button" className="underline" onClick={() => { openExisting(existing.id); }}>
                      {existing.status === 'invited' ? '招待リンクを出し直す' : 'アカウントの詳細を開く'}
                    </button>
                  </span>
                ) : formError
              }
            >
              <TextInput
                id="inv-e"
                type="text"
                inputMode="email"
                value={email}
                onChange={(e) => { setEmail(e.target.value); }}
                onBlur={() => { if (email.trim() !== '') setTouched(true); }}
                placeholder="例: saiyou@example.com"
              />
            </Field>
            <Field label="表示名（任意）" htmlFor="inv-n">
              <TextInput id="inv-n" value={name} onChange={(e) => { setName(e.target.value); }} placeholder="例: 採用担当 木村" />
            </Field>
            <p className="text-sm text-pt-muted">パスワードは本人が招待リンクから決めます。社内の人には分からない作りです。</p>
            <div className="flex gap-2">
              <Button variant="primary" onClick={submit}>招待リンクを発行する</Button>
              <Button variant="ghost" onClick={() => { nav.go('admin-accounts'); }}>やめる</Button>
            </div>
          </Card>
        ) : (
          <>
            <Notice tone="ok" title="招待リンクを発行しました">
              {issued.email}（{customerName(issued.customerId)}）。リンクは {formatShortDateTime(issued.expiresAt)} まで有効で、1 回だけ使えます。この画面を閉じると再表示できません。
            </Notice>
            <Card className="space-y-2 p-5">
              <p className="text-sm font-medium">招待リンク</p>
              <div className="flex flex-wrap items-center gap-2">
                <code className="min-w-0 flex-1 break-all rounded bg-pt-bg px-3 py-2 text-sm">{issued.link}</code>
                <CopyButton text={issued.link} label="リンクをコピー" />
              </div>
            </Card>
            <Card className="space-y-2 p-5">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium">顧客に送るメールの文面</p>
                <CopyButton text={mailText(issued)} label="文面をコピー" />
              </div>
              <TextArea readOnly value={mailText(issued)} rows={mailText(issued).split('\n').length + 1} className="!min-h-0 text-sm" />
            </Card>
            <div className="flex gap-2">
              <Button variant="primary" onClick={() => { nav.go('admin-accounts'); }}>一覧に戻る</Button>
              <Button onClick={reset}>続けて招待する</Button>
            </div>
          </>
        )}
      </div>
    </AccountsAdminLayout>
  );
}
