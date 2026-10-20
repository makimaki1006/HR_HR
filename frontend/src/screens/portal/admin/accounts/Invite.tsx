import { useEffect, useState } from 'react';
import { useData } from '../../data';
import { usePreviewNav } from '../../preview';
import { Button, Field, Select, TextArea, TextInput, formatShortDateTime } from '../../ui/kit';
import { AdminPage } from '../customers/shared';
import { actions as adminActions, useAdminState } from '../customers/state';
import { customerName, findByEmail, invite, selectAccount, LOGIN_URL } from './store';
import { STATUS_LABEL, CopyButton } from './shared';

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
  const d = useData();
  const admin = useAdminState();
  const active = d.customers.filter((c) => !c.disabled);
  // 顧客の詳細の「招待」から来たときは、その顧客を選んでおく。
  const [customerId, setCustomerId] = useState(active.find((c) => c.id === admin.inviteCustomerId)?.id ?? active[0]?.id ?? '');
  useEffect(() => { adminActions.setInviteCustomer(null); }, []);
  const [email, setEmail] = useState('');
  const [name, setName] = useState('');
  const [issued, setIssued] = useState<Issued | null>(null);
  const [touched, setTouched] = useState(false);
  const formOk = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim());
  const domainOf = (v: string): string => v.trim().toLowerCase().split('@')[1] ?? '';
  const knownDomains = new Set(d.accounts.filter((a) => a.customerId === customerId).map((a) => domainOf(a.email)));
  const otherDomain = formOk && knownDomains.size > 0 && !knownDomains.has(domainOf(email));
  const existing = email.trim() !== '' ? findByEmail(email) : undefined;
  const formError = touched && email.trim() !== '' && !formOk ? 'メールアドレスの形になっていません。' : touched && email.trim() === '' ? 'メールアドレスを入力してください。' : null;

  const submit = (): void => {
    setTouched(true);
    if (!formOk || existing || customerId === '') return;
    const r = invite(customerId, email.trim(), name.trim());
    setIssued({ link: r.link, expiresAt: r.expiresAt, email: email.trim(), customerId, displayName: name.trim() });
  };
  const reset = (): void => { setIssued(null); setEmail(''); setName(''); setTouched(false); };
  const openExisting = (id: string): void => { selectAccount(id); nav.go('admin-account-detail'); };

  return (
    <AdminPage title="招待する" current="accounts" back={{ label: 'アカウント', to: 'admin-accounts' }}>
      <div className="max-w-xl space-y-4">
        {!issued ? (
          <div className="space-y-4 rounded-lg border border-pt-line bg-pt-surface p-5">
            <Field label="顧客" htmlFor="inv-c">
              <Select id="inv-c" value={customerId} onChange={(e) => { setCustomerId(e.target.value); }}>
                {active.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
              </Select>
            </Field>
            <Field
              label="メールアドレス"
              htmlFor="inv-e"
              hint="ログインと確認コードの送り先になる。"
              error={
                existing ? (
                  <span>
                    すでに登録されている（{customerName(existing.customerId)}・{STATUS_LABEL[existing.status]}）。{' '}
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
            {otherDomain && <p className="-mt-2 text-xs text-pt-warn">ほかのアカウントと違うドメイン</p>}
            <Field label="表示名（任意）" htmlFor="inv-n">
              <TextInput id="inv-n" value={name} onChange={(e) => { setName(e.target.value); }} placeholder="例: 採用担当 木村" />
            </Field>
            <p className="text-xs text-pt-muted">パスワードは本人が招待リンクから決める。社内の人には分からない。</p>
            <div className="flex gap-2">
              <Button variant="primary" onClick={submit}>招待リンクを発行する</Button>
              <Button variant="ghost" onClick={() => { nav.go('admin-accounts'); }}>やめる</Button>
            </div>
          </div>
        ) : (
          <>
            <p className="text-[13px]">
              <span className="mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-pt-ok align-middle" />
              <span className="font-medium">招待リンクを発行した</span>
              <span className="ml-2 text-pt-muted">{issued.email}（{customerName(issued.customerId)}）／ <span className="pt-num">{formatShortDateTime(issued.expiresAt)}</span> まで有効、1 回だけ使える。この画面を閉じると再表示できない。</span>
            </p>
            <div className="flex gap-2">
              <CopyButton text={mailText(issued)} label="文面をコピー" primary />
              <Button onClick={() => { nav.go('admin-accounts'); }}>一覧に戻る</Button>
              <Button variant="ghost" onClick={reset}>続けて招待する</Button>
            </div>
            <section className="space-y-2 rounded-lg border border-pt-line bg-pt-surface p-4">
              <p className="text-[13px] font-medium">招待リンク</p>
              <div className="flex flex-wrap items-center gap-2">
                <code className="min-w-0 flex-1 break-all rounded bg-pt-sunken px-3 py-2 text-xs">{issued.link}</code>
                <CopyButton text={issued.link} label="リンクをコピー" />
              </div>
            </section>
            <section className="space-y-2 rounded-lg border border-pt-line bg-pt-surface p-4">
              <p className="text-[13px] font-medium">顧客に送るメールの文面</p>
              <TextArea readOnly value={mailText(issued)} rows={8} className="!min-h-0 text-[13px]" />
            </section>
          </>
        )}
      </div>
    </AdminPage>
  );
}
