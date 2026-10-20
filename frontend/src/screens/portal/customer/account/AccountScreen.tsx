import { useEffect, useRef, useState, type ReactNode } from 'react';
import { CURRENT_ACCOUNT_ID, LOGIN_URL, devices as initialDevices } from '../../fixtures';
import { ContactLine } from '../../ui/contact';
import { ROUTES, usePreviewNav } from '../../preview';
import { usePortalStore } from '../../store';
import { Badge, Button, Card, Field, Notice, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { CustomerLayout } from '../../ui/layouts';
import type { TrustedDevice } from '../../types';

/** 試作で使う入力例。 */
const SAMPLE_PASSWORD = 'sample-pass-2026';
const SAMPLE_CODE = '123456';
const FAIL_TEXT = '保存できませんでした。通信を確かめて、もう一度お試しください。何も変わっていません。';

const dateOnly = (iso: string) =>
  new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', year: 'numeric', month: 'numeric', day: 'numeric' }).format(new Date(iso));

/** 約 0.5 秒の「保存中」を作る。failing のときは失敗で終わる。 */
function useSave(failing: boolean) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | null>(null);
  useEffect(() => () => { if (timer.current !== null) window.clearTimeout(timer.current); }, []);
  const run = (onOk: () => void, failText: string = FAIL_TEXT) => {
    setBusy(true);
    setError(null);
    timer.current = window.setTimeout(() => {
      setBusy(false);
      if (failing) setError(failText);
      else onOk();
    }, 500);
  };
  return { busy, error, run, clear: () => { setError(null); } };
}

function Section({ title, note, children }: { title: string; note?: ReactNode; children: ReactNode }) {
  return (
    <Card className="p-4 sm:p-5">
      <h2 className="text-base font-semibold text-pt-ink">{title}</h2>
      {note && <p className="mt-1 text-sm text-pt-muted">{note}</p>}
      <div className="mt-4">{children}</div>
    </Card>
  );
}

function Fold({ title, children }: { title: string; children: ReactNode }) {
  return (
    <details className="group rounded-lg border border-pt-line bg-pt-surface">
      <summary className="flex min-h-11 cursor-pointer list-none items-center justify-between px-4 py-3 text-sm font-medium text-pt-ink sm:px-5">
        {title}
        <span className="text-pt-faint transition-transform group-open:rotate-90" aria-hidden="true">›</span>
      </summary>
      <div className="border-t border-pt-line px-4 py-4 text-sm leading-relaxed text-pt-ink sm:px-5">{children}</div>
    </details>
  );
}

function DeviceSection({ list, setList, failing }: { list: TrustedDevice[]; setList: (f: (l: TrustedDevice[]) => TrustedDevice[]) => void; failing: boolean }) {
  const [confirm, setConfirm] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const save = useSave(failing);
  const others = list.filter((d) => !d.current);

  const remove = (id: string) => {
    save.run(() => {
      setList((l) => l.filter((d) => d.id !== id));
      setConfirm(null);
      setMessage('この端末を解除しました。次に使うときは確認コードが必要です。');
    });
  };
  const removeOthers = () => {
    save.run(() => {
      setList((l) => l.filter((d) => d.current));
      setConfirm(null);
      setMessage('この端末以外をすべて解除しました。');
    });
  };
  const cancel = () => { setConfirm(null); save.clear(); };

  return (
    <Section title="覚えている端末" note="一度確認コードを入れた端末は、最後に使ってから 90 日間、コードなしでログインできます。">
      <ul className="divide-y divide-pt-line">
        {list.map((d) => (
          <li key={d.id} className="py-3 first:pt-0">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <div className="min-w-0 flex-1">
                <p className="flex flex-wrap items-center gap-2 text-base font-medium text-pt-ink">
                  {d.label}
                  {d.current && <Badge tone="ok">この端末</Badge>}
                </p>
                <p className="mt-0.5 text-sm text-pt-muted">
                  最後に使った日時 {formatShortDateTime(d.lastUsedAt)} ／ {dateOnly(d.expiresAt)} まで覚えています
                </p>
              </div>
              {!d.current && confirm !== d.id && (
                <Button className="min-h-11 min-w-16" onClick={() => { setConfirm(d.id); setMessage(null); save.clear(); }}>解除</Button>
              )}
            </div>
            {confirm === d.id && (
              <div className="mt-2">
                <Notice tone="warn" title={`${d.label}を解除しますか`}>
                  解除すると、この端末では次回から確認コードが必要になります。
                  <div className="mt-3 flex gap-2">
                    <Button className="min-h-11" variant="danger" disabled={save.busy} onClick={() => { remove(d.id); }}>{save.busy ? '解除中…' : '解除する'}</Button>
                    <Button className="min-h-11" disabled={save.busy} onClick={cancel}>やめる</Button>
                  </div>
                </Notice>
                {save.error && <div className="mt-2"><Notice tone="danger">{save.error}</Notice></div>}
              </div>
            )}
          </li>
        ))}
      </ul>
      {message && <div className="mt-3"><Notice tone="ok">{message}</Notice></div>}
      {others.length >= 2 && (
        <div className="mt-4 border-t border-pt-line pt-3">
          {confirm === 'others' ? (
            <>
              <Notice tone="warn" title="この端末以外をすべて解除しますか">
                ほかの端末は、次回から確認コードが必要になります。
                <div className="mt-3 flex gap-2">
                  <Button className="min-h-11" variant="danger" disabled={save.busy} onClick={removeOthers}>{save.busy ? '解除中…' : 'すべて解除する'}</Button>
                  <Button className="min-h-11" disabled={save.busy} onClick={cancel}>やめる</Button>
                </div>
              </Notice>
              {save.error && <div className="mt-2"><Notice tone="danger">{save.error}</Notice></div>}
            </>
          ) : (
            <Button variant="ghost" className="min-h-11 text-pt-danger hover:bg-pt-danger-soft" onClick={() => { setConfirm('others'); setMessage(null); save.clear(); }}>この端末以外をすべて解除</Button>
          )}
        </div>
      )}
    </Section>
  );
}

function CodeDestination({ initial, loginEmail, failing }: { initial: string | null; loginEmail: string; failing: boolean }) {
  const [saved, setSaved] = useState<string | null>(initial);
  const [stage, setStage] = useState<'idle' | 'input' | 'code'>('idle');
  const [value, setValue] = useState('');
  const [code, setCode] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [codeError, setCodeError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [resent, setResent] = useState(false);
  const save = useSave(failing);

  const reset = () => { setStage('idle'); setValue(''); setCode(''); setError(null); setCodeError(null); setResent(false); save.clear(); };

  const sendCode = () => {
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value)) {
      setError('メールアドレスの形で入力してください。');
      return;
    }
    if (value === loginEmail) {
      setError('ログインのメールアドレスと同じです。別のアドレスを入れてください。');
      return;
    }
    setError(null);
    save.run(() => { setStage('code'); setResent(false); }, 'コードを送れませんでした。通信を確かめて、もう一度お試しください。');
  };
  const resend = () => {
    save.run(() => { setResent(true); }, 'コードを送れませんでした。通信を確かめて、もう一度お試しください。');
  };
  const verify = () => {
    if (code.trim() !== SAMPLE_CODE) {
      setCodeError('コードが違います。メールに届いた 6 桁の数字を入れてください。');
      return;
    }
    setCodeError(null);
    save.run(() => {
      setSaved(value);
      setMessage('追加しました。次の確認コードから、このアドレスにも届きます。ログインのメールにも、追加したことをお知らせしました。');
      reset();
    });
  };
  const doDelete = () => {
    save.run(() => {
      setSaved(null);
      setConfirmDelete(false);
      setMessage('追加の送り先を削除しました。確認コードは、ログインのメールにだけ届きます。');
    });
  };

  return (
    <Section title="確認コードの送り先" note="新しい端末でログインするときに、6 桁のコードをメールでお送りします。">
      <ul className="divide-y divide-pt-line rounded-md border border-pt-line">
        <li className="flex flex-wrap items-center gap-2 px-3 py-2.5">
          <span className="min-w-0 flex-1 break-all text-base text-pt-ink">{loginEmail}</span>
          <Badge tone="neutral">ログインのメール</Badge>
        </li>
        {saved && (
          <li className="flex flex-wrap items-center gap-2 px-3 py-2.5">
            <span className="min-w-0 flex-1 break-all text-base text-pt-ink">{saved}</span>
            <Badge tone="info">追加の送り先</Badge>
            {!confirmDelete && <Button className="min-h-11 min-w-16" onClick={() => { setConfirmDelete(true); setMessage(null); save.clear(); }}>削除</Button>}
          </li>
        )}
      </ul>
      {confirmDelete && (
        <div className="mt-3">
          <Notice tone="warn" title="追加の送り先を削除しますか">
            削除すると、確認コードはログインのメールにだけ届きます。
            <div className="mt-3 flex gap-2">
              <Button className="min-h-11" variant="danger" disabled={save.busy} onClick={doDelete}>{save.busy ? '削除中…' : '削除する'}</Button>
              <Button className="min-h-11" disabled={save.busy} onClick={() => { setConfirmDelete(false); save.clear(); }}>やめる</Button>
            </div>
          </Notice>
          {save.error && <div className="mt-2"><Notice tone="danger">{save.error}</Notice></div>}
        </div>
      )}
      {message && <div className="mt-3"><Notice tone="ok">{message}</Notice></div>}
      {!saved && stage === 'idle' && (
        <div className="mt-3">
          <Button className="min-h-11" onClick={() => { setStage('input'); setMessage(null); }}>スマホで読めるアドレスを追加</Button>
        </div>
      )}
      {stage === 'input' && (
        <div className="mt-3 flex flex-col gap-3">
          <Field label="追加するメールアドレス" hint="外出先でも読めるアドレスにしてください。このアドレスへ確認コードを送り、届いたことを確かめてから登録します。" error={error} htmlFor="extra-mail">
            <TextInput id="extra-mail" type="email" inputMode="email" autoComplete="email" value={value} placeholder="例: yamada.mobile@example.com" onChange={(e) => { setValue(e.target.value); }} />
          </Field>
          {save.error && <Notice tone="danger">{save.error}</Notice>}
          <div className="flex gap-2">
            <Button className="min-h-11" variant="primary" disabled={save.busy} onClick={sendCode}>{save.busy ? '送っています…' : '確認コードを送る'}</Button>
            <Button className="min-h-11" disabled={save.busy} onClick={reset}>やめる</Button>
          </div>
        </div>
      )}
      {stage === 'code' && (
        <div className="mt-3 flex flex-col gap-3">
          <Notice tone="info">{value} に 6 桁の確認コードを送りました。</Notice>
          <Field label="確認コード" error={codeError} htmlFor="extra-code" hint={`試作用の入力例: ${SAMPLE_CODE}`}>
            <TextInput id="extra-code" inputMode="numeric" autoComplete="one-time-code" maxLength={6} value={code} placeholder="6 桁の数字" onChange={(e) => { setCode(e.target.value); setCodeError(null); }} />
          </Field>
          {resent && <p className="text-sm text-pt-ok">コードを再送しました。</p>}
          {save.error && <Notice tone="danger">{save.error}</Notice>}
          <div className="flex flex-wrap gap-2">
            <Button className="min-h-11" variant="primary" disabled={save.busy} onClick={verify}>{save.busy ? '確かめています…' : '登録する'}</Button>
            <Button className="min-h-11" disabled={save.busy} onClick={resend}>コードを再送</Button>
            <Button className="min-h-11" variant="ghost" disabled={save.busy} onClick={reset}>やめる</Button>
          </div>
        </div>
      )}
    </Section>
  );
}

function PhoneUse() {
  const url = LOGIN_URL;
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard.writeText(url).catch(() => undefined);
    setCopied(true);
  };
  return (
    <Section
      title="スマホでも使う"
      note={
        <>
          <span className="hidden sm:inline">外出先でもスマホから応募者の確認と更新ができます。下のアドレスをスマホのブラウザで開き、ブックマークかホーム画面に追加しておくと便利です。</span>
          <span className="sm:hidden">ホーム画面に追加すると、次からすぐ開けます。</span>
        </>
      }
    >
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <code className="block min-w-0 flex-1 truncate rounded-md border border-pt-line bg-pt-bg px-3 py-2 text-sm text-pt-ink">{url}</code>
        <Button className="min-h-11 sm:min-w-40" onClick={copy}>{copied ? 'コピーしました' : 'アドレスをコピー'}</Button>
      </div>
      <p className="mt-2 text-xs text-pt-muted">スマホではじめてログインするときは、確認コードが必要です。</p>
    </Section>
  );
}

function Profile({ email, name, setName, failing }: { email: string; name: string; setName: (n: string) => void; failing: boolean }) {
  const [draft, setDraft] = useState(name);
  const [done, setDone] = useState(false);
  const save = useSave(failing);
  return (
    <Section title="アカウント">
      <div className="flex flex-col gap-4">
        <Field label="ログインのメールアドレス" hint="変更したいときは、弊社の担当者にご連絡ください。">
          <TextInput value={email} readOnly disabled />
        </Field>
        <Field label="表示名" hint="画面の上と、変更した人として記録に表示されます。" htmlFor="display-name">
          <TextInput id="display-name" value={draft} onChange={(e) => { setDraft(e.target.value); setDone(false); save.clear(); }} />
        </Field>
        {save.error && <Notice tone="danger">{save.error}</Notice>}
        <div className="flex items-center gap-3">
          <Button className="min-h-11" variant="primary" disabled={draft === name || draft.trim() === '' || save.busy} onClick={() => { save.run(() => { setName(draft.trim()); setDone(true); }); }}>{save.busy ? '保存中…' : '表示名を保存'}</Button>
          {done && <span className="text-sm text-pt-ok">保存しました</span>}
        </div>
      </div>
    </Section>
  );
}

function PwInput({ id, value, onChange, auto }: { id: string; value: string; onChange: (v: string) => void; auto: string }) {
  const [shown, setShown] = useState(false);
  return (
    <div className="flex gap-2">
      <TextInput id={id} type={shown ? 'text' : 'password'} autoComplete={auto} value={value} onChange={(e) => { onChange(e.target.value); }} />
      <Button className="min-h-11 min-w-16 shrink-0" aria-pressed={shown} onClick={() => { setShown((s) => !s); }}>{shown ? '隠す' : '表示'}</Button>
    </div>
  );
}

function PasswordSection({ failing, onChanged }: { failing: boolean; onChanged: () => void }) {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [again, setAgain] = useState('');
  const [errors, setErrors] = useState<{ current?: string; next?: string; again?: string }>({});
  const [open, setOpen] = useState(false);
  const save = useSave(failing);

  const submit = () => {
    const e: typeof errors = {};
    if (current === '') e.current = '今のパスワードを入力してください。';
    if (next.length < 12) e.next = '12 文字以上で入力してください。';
    else if (next === current) e.next = '今のパスワードと違うものにしてください。';
    if (again !== next) e.again = '新しいパスワードと同じものを入力してください。';
    setErrors(e);
    if (Object.keys(e).length > 0) return;
    save.run(() => {
      if (current !== SAMPLE_PASSWORD) {
        setErrors({ current: '今のパスワードが違います。' });
        return;
      }
      setOpen(false);
      setCurrent('');
      setNext('');
      setAgain('');
      onChanged();
    }, '変更できませんでした。通信を確かめて、もう一度お試しください。パスワードは変わっていません。');
  };

  return (
    <Section title="パスワードの変更">
      {!open && <Button className="min-h-11" onClick={() => { setOpen(true); }}>パスワードを変更する</Button>}
      {open && (
        <div className="flex flex-col gap-4">
          <Field label="今のパスワード" error={errors.current} hint={`試作用の入力例: ${SAMPLE_PASSWORD}`} htmlFor="pw-current">
            <PwInput id="pw-current" auto="current-password" value={current} onChange={setCurrent} />
          </Field>
          <Field label="新しいパスワード" hint="12 文字以上。ほかのサービスと同じものは避けてください。" error={errors.next} htmlFor="pw-next">
            <PwInput id="pw-next" auto="new-password" value={next} onChange={setNext} />
            <p className={cx('text-xs', next.length >= 12 ? 'text-pt-ok' : 'text-pt-muted')}>{next.length} / 12 文字</p>
          </Field>
          <Field label="新しいパスワード（確認）" error={errors.again} htmlFor="pw-again">
            <PwInput id="pw-again" auto="new-password" value={again} onChange={setAgain} />
          </Field>
          <p className="text-sm text-pt-muted">変更すると、この端末以外で覚えていたログインは解除されます。</p>
          {save.error && <Notice tone="danger">{save.error}</Notice>}
          <div className="flex gap-2">
            <Button className="min-h-11" variant="primary" disabled={save.busy} onClick={submit}>{save.busy ? '変更中…' : '変更する'}</Button>
            <Button className="min-h-11" disabled={save.busy} onClick={() => { setOpen(false); setErrors({}); save.clear(); }}>やめる</Button>
          </div>
        </div>
      )}
    </Section>
  );
}

export function AccountScreen({ passwordDone = false, failing = false }: { passwordDone?: boolean; failing?: boolean }) {
  const { go } = usePreviewNav();
  const { account, customerName } = usePortalStore();
  const [devices, setDevices] = useState<TrustedDevice[]>(() =>
    initialDevices.filter((d) => d.accountId === CURRENT_ACCOUNT_ID && (!passwordDone || d.current)),
  );
  const [name, setName] = useState(account.displayName);
  const [done, setDone] = useState(passwordDone);
  const bannerRef = useRef<HTMLDivElement>(null);
  const label = name || account.email;

  useEffect(() => {
    if (done) bannerRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, [done]);

  return (
    <CustomerLayout
      customerName={customerName}
      accountLabel={label}
      current="account"
      wide={false}
      onNavigate={(to) => { go(to === 'list' ? ROUTES.list : to === 'account' ? ROUTES.account : ROUTES.loggedOut); }}
    >
      <h1 className="mb-4 text-xl font-semibold text-pt-ink">アカウントの設定</h1>
      <div ref={bannerRef} className="scroll-mt-20">
        {done && (
          <div className="mb-4">
            <Notice
              tone="ok"
              title="パスワードを変更しました"
              action={<Button className="min-h-11" onClick={() => { setDone(false); }}>閉じる</Button>}
            >
              ほかの端末で覚えていたログインは解除しました。ほかの端末では、次回から確認コードが必要です。この端末はそのまま使えます。
            </Notice>
          </div>
        )}
      </div>
      <div className="flex flex-col gap-4">
        <DeviceSection list={devices} setList={setDevices} failing={failing} />
        <CodeDestination initial={account.extraCodeEmail} loginEmail={account.email} failing={failing} />
        <Profile email={account.email} name={name} setName={setName} failing={failing} />
        <PasswordSection
          failing={failing}
          onChanged={() => { setDevices((l) => l.filter((d) => d.current)); setDone(true); }}
        />
        <PhoneUse />
        <Fold title="利用規約">
          <p>この画面は、弊社がご案内した担当者の方だけがお使いいただけます。ログインのメールアドレスとパスワードは、ほかの方と共有しないでください。</p>
          <p className="mt-2">応募者の情報は、採用の連絡と選考の管理にだけお使いください。担当者が変わったときは、弊社にご連絡ください。</p>
        </Fold>
        <Card className="p-4 sm:p-5">
          <h2 className="text-base font-semibold text-pt-ink">困ったとき</h2>
          <p className="mt-2 text-sm leading-relaxed text-pt-ink">ログインできないとき、メールアドレスを変えたいとき、担当者を追加したいときは、弊社の担当者にご連絡ください。</p>
          <p className="mt-2 text-sm text-pt-ink"><ContactLine /></p>
        </Card>
        <div className="sm:hidden">
          <Button className="min-h-11 w-full" onClick={() => { go(ROUTES.loggedOut); }}>ログアウト</Button>
        </div>
      </div>
    </CustomerLayout>
  );
}
