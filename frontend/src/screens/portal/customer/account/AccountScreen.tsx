import { useEffect, useRef, useState, type ReactNode } from 'react';
import { LOGIN_URL, SUPPORT, devices as initialDevices } from '../../fixtures';
import { actions, currentCustomer, currentSheet, getData, log, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { AppShell } from '../../ui/shell';
import { Badge, Button, Dialog, Field, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { IconCheck, IconChevronRight, IconLogout } from '../../ui/icons';
import { ContactLines, IconCopy, PasswordInput } from '../auth/parts';
import type { PortalAccount, TrustedDevice } from '../../types';

/** 試作で使う入力例。 */
const SAMPLE_PASSWORD = 'sample-pass-2026';
const SAMPLE_CODE = '123456';
const FAIL_TEXT = '保存できませんでした。通信を確かめて、もう一度お試しください。何も変わっていません。';
const SEND_FAIL = 'コードを送れませんでした。通信を確かめて、もう一度お試しください。';

/** 入力欄・ボタンの高さ。スマホは 44px、PC は 36px / 32px。 */
const FIELD = 'sm:h-9';
const BTN = 'sm:h-8';

const dateOnly = (iso: string) =>
  new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', year: 'numeric', month: 'numeric', day: 'numeric' }).format(new Date(iso));

function patchAccount(id: string, patch: Partial<PortalAccount>): void {
  actions.updateAccounts(getData().accounts.map((a) => (a.id === id ? { ...a, ...patch } : a)));
}

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

/** 設定の 1 行。左に項目名と 1 行の説明、右に値と操作。 */
function Row({ title, desc, children }: { title: string; desc?: string; children: ReactNode }) {
  return (
    <section className="grid gap-x-8 gap-y-2.5 border-b border-pt-line py-4 last:border-b-0 sm:grid-cols-[200px_minmax(0,1fr)]">
      <div>
        <h2 className="text-[13px] font-semibold text-pt-ink">{title}</h2>
        {desc && <p className="mt-0.5 text-xs leading-relaxed text-pt-muted">{desc}</p>}
      </div>
      <div className="min-w-0 text-[13px]">{children}</div>
    </section>
  );
}

function Saved({ children }: { children: ReactNode }) {
  return <p className="mt-2 flex items-start gap-1.5 text-xs text-pt-ok"><IconCheck size={14} className="mt-px shrink-0" />{children}</p>;
}

function Failed({ children }: { children: ReactNode }) {
  return <p role="alert" className="mt-2 text-xs text-pt-danger">{children}</p>;
}

function DeviceRow({ list, setList, failing }: { list: TrustedDevice[]; setList: (f: (l: TrustedDevice[]) => TrustedDevice[]) => void; failing: boolean }) {
  const [confirm, setConfirm] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const save = useSave(failing);
  const others = list.filter((d) => !d.current);
  const target = list.find((d) => d.id === confirm);

  const close = () => { setConfirm(null); save.clear(); };
  const remove = () => {
    save.run(() => {
      if (confirm === 'others') {
        setList((l) => l.filter((d) => d.current));
        setMessage('この端末以外をすべて解除しました。');
        log({ kind: 'device_removed', detail: `この端末以外の ${String(others.length)} 台の記憶を解除` });
      } else {
        setList((l) => l.filter((d) => d.id !== confirm));
        setMessage('解除しました。次に使うときは確認コードが必要です。');
        log({ kind: 'device_removed', detail: `「${target?.label ?? ''}」の記憶を解除` });
      }
      setConfirm(null);
    });
  };

  return (
    <Row title="覚えている端末" desc="確認コードなしで 90 日間ログインできます。">
      {list.length === 0 ? (
        <p className="text-pt-muted">覚えている端末はありません。</p>
      ) : (
        <ul className="divide-y divide-pt-line rounded-md border border-pt-line bg-pt-surface">
          {list.map((d) => (
            <li key={d.id} className="flex items-center gap-3 px-3 py-2">
              <div className="min-w-0 flex-1">
                <p className="flex flex-wrap items-center gap-2 font-medium text-pt-ink">
                  {d.label}
                  {d.current && <Badge tone="ok">この端末</Badge>}
                </p>
                <p className="mt-0.5 text-xs text-pt-muted"><span className="pt-num">{formatShortDateTime(d.lastUsedAt)}</span> に使用 ／ <span className="pt-num">{dateOnly(d.expiresAt)}</span> まで</p>
              </div>
              {!d.current && <Button className={cx('min-w-14', BTN)} onClick={() => { setConfirm(d.id); setMessage(null); save.clear(); }}>解除</Button>}
            </li>
          ))}
        </ul>
      )}
      {others.length >= 2 && (
        <Button variant="ghost" className={cx('mt-1.5 -ml-2.5 text-pt-danger hover:bg-pt-danger-soft', BTN)} onClick={() => { setConfirm('others'); setMessage(null); save.clear(); }}>この端末以外をすべて解除</Button>
      )}
      {message && <Saved>{message}</Saved>}
      <Dialog
        open={confirm !== null}
        width="sm"
        title={confirm === 'others' ? 'この端末以外をすべて解除' : `「${target?.label ?? ''}」の記憶を解除`}
        onClose={close}
        footer={
          <>
            <Button disabled={save.busy} onClick={close}>やめる</Button>
            <Button variant="danger" disabled={save.busy} onClick={remove}>{save.busy ? '解除中…' : '解除する'}</Button>
          </>
        }
      >
        <p className="text-[13px] text-pt-ink">解除した端末では、次回から確認コードが必要になります。</p>
        {save.error && <Failed>{save.error}</Failed>}
      </Dialog>
    </Row>
  );
}

function CodeDestinationRow({ account, failing }: { account: PortalAccount; failing: boolean }) {
  const saved = account.extraCodeEmail;
  const loginEmail = account.email;
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
    save.run(() => { setStage('code'); setResent(false); log({ kind: 'code_sent', detail: '追加の送り先の確認コードを送信' }); }, SEND_FAIL);
  };
  const resend = () => { save.run(() => { setResent(true); log({ kind: 'code_sent', detail: '追加の送り先の確認コードを送り直し' }); }, SEND_FAIL); };
  const verify = () => {
    if (code.trim() !== SAMPLE_CODE) {
      setCodeError('コードが違います。メールに届いた 6 桁の数字を入れてください。');
      return;
    }
    setCodeError(null);
    save.run(() => {
      patchAccount(account.id, { extraCodeEmail: value });
      log({ kind: 'save', detail: `確認コードの追加の送り先を登録（${value}）` });
      setMessage('追加しました。次の確認コードから、このアドレスにも届きます。');
      reset();
    });
  };
  const doDelete = () => {
    save.run(() => {
      patchAccount(account.id, { extraCodeEmail: null });
      log({ kind: 'save', detail: `確認コードの追加の送り先を削除（${saved ?? ''}）` });
      setConfirmDelete(false);
      setMessage('削除しました。確認コードはログインのメールにだけ届きます。');
    });
  };

  return (
    <Row title="確認コードの送り先" desc="新しい端末でのログインで 6 桁のコードを送ります。">
      <ul className="divide-y divide-pt-line rounded-md border border-pt-line bg-pt-surface">
        <li className="flex flex-wrap items-center gap-2 px-3 py-2">
          <span className="min-w-0 flex-1 break-all text-pt-ink">{loginEmail}</span>
          <Badge tone="neutral">ログインのメール</Badge>
        </li>
        {saved && (
          <li className="flex flex-wrap items-center gap-2 px-3 py-2">
            <span className="min-w-0 flex-1 break-all text-pt-ink">{saved}</span>
            <Badge tone="info">追加</Badge>
            <Button className={cx('min-w-14', BTN)} onClick={() => { setConfirmDelete(true); setMessage(null); save.clear(); }}>削除</Button>
          </li>
        )}
      </ul>
      {message && <Saved>{message}</Saved>}
      {!saved && stage === 'idle' && (
        <Button className={cx('mt-2.5', BTN)} onClick={() => { setStage('input'); setMessage(null); }}>スマホで読めるアドレスを追加</Button>
      )}
      {stage === 'input' && (
        <div className="mt-3 flex max-w-sm flex-col gap-3">
          <Field label="追加するメールアドレス" hint="外出先でも読めるアドレスにしてください。コードを送って確かめます。" error={error} htmlFor="extra-mail">
            <TextInput id="extra-mail" type="email" inputMode="email" autoComplete="email" className={FIELD} value={value} placeholder="例: yamada.mobile@example.com" onChange={(e) => { setValue(e.target.value); }} />
          </Field>
          {save.error && <Failed>{save.error}</Failed>}
          <div className="flex gap-2">
            <Button variant="primary" className={BTN} disabled={save.busy} onClick={sendCode}>{save.busy ? '送っています…' : '確認コードを送る'}</Button>
            <Button className={BTN} disabled={save.busy} onClick={reset}>やめる</Button>
          </div>
        </div>
      )}
      {stage === 'code' && (
        <div className="mt-3 flex max-w-sm flex-col gap-3">
          <p className="text-xs text-pt-muted"><span className="text-pt-ink">{value}</span> に 6 桁のコードを送りました。</p>
          <Field label="確認コード" error={codeError} htmlFor="extra-code" hint={`試作用の入力例: ${SAMPLE_CODE}`}>
            <TextInput id="extra-code" inputMode="numeric" autoComplete="one-time-code" maxLength={6} className={cx(FIELD, 'pt-num')} value={code} placeholder="6 桁の数字" onChange={(e) => { setCode(e.target.value); setCodeError(null); }} />
          </Field>
          {resent && <p className="text-xs text-pt-ok">コードを送り直しました。</p>}
          {save.error && <Failed>{save.error}</Failed>}
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" className={BTN} disabled={save.busy} onClick={verify}>{save.busy ? '確かめています…' : '登録する'}</Button>
            <Button className={BTN} disabled={save.busy} onClick={resend}>コードを送り直す</Button>
            <Button variant="ghost" className={BTN} disabled={save.busy} onClick={reset}>やめる</Button>
          </div>
        </div>
      )}
      <Dialog
        open={confirmDelete}
        width="sm"
        title="追加の送り先を削除"
        onClose={() => { setConfirmDelete(false); save.clear(); }}
        footer={
          <>
            <Button disabled={save.busy} onClick={() => { setConfirmDelete(false); save.clear(); }}>やめる</Button>
            <Button variant="danger" disabled={save.busy} onClick={doDelete}>{save.busy ? '削除中…' : '削除する'}</Button>
          </>
        }
      >
        <p className="text-[13px] text-pt-ink">削除すると、確認コードはログインのメールにだけ届きます。</p>
        {save.error && <Failed>{save.error}</Failed>}
      </Dialog>
    </Row>
  );
}

function ProfileRow({ account, failing }: { account: PortalAccount; failing: boolean }) {
  const name = account.displayName;
  const [draft, setDraft] = useState(name);
  const [done, setDone] = useState(false);
  const save = useSave(failing);
  const changed = draft.trim() !== name && draft.trim() !== '';
  return (
    <Row title="プロフィール" desc="表示名は、変更した人として記録に残ります。">
      <div className="flex max-w-sm flex-col gap-3">
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
          <span className="break-all text-pt-ink">{account.email}</span>
          <span className="text-xs text-pt-faint">変えるときは担当者へ</span>
        </div>
        <Field label="表示名" htmlFor="display-name">
          <TextInput id="display-name" className={FIELD} value={draft} onChange={(e) => { setDraft(e.target.value); setDone(false); save.clear(); }} />
        </Field>
        {save.error && <Failed>{save.error}</Failed>}
        <div className="flex items-center gap-3">
          {changed && (
            <Button
              variant="primary"
              className={BTN}
              disabled={save.busy}
              onClick={() => {
                save.run(() => {
                  const next = draft.trim();
                  patchAccount(account.id, { displayName: next });
                  log({ kind: 'save', detail: `表示名を「${name || '（未設定）'}」から「${next}」に変更`, before: name, after: next });
                  setDraft(next);
                  setDone(true);
                });
              }}
            >
              {save.busy ? '保存中…' : '保存'}
            </Button>
          )}
          {done && !changed && <span className="flex items-center gap-1 text-xs text-pt-ok"><IconCheck size={14} />保存しました</span>}
        </div>
      </div>
    </Row>
  );
}

function PasswordRow({ failing, initialDone, onChanged }: { failing: boolean; initialDone: boolean; onChanged: () => void }) {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [again, setAgain] = useState('');
  const [errors, setErrors] = useState<{ current?: string; next?: string; again?: string }>({});
  const [open, setOpen] = useState(false);
  const [done, setDone] = useState(initialDone);
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
      setDone(true);
      log({ kind: 'save', detail: 'パスワードを変更（ほかの端末の記憶を解除）' });
      onChanged();
    }, '変更できませんでした。通信を確かめて、もう一度お試しください。パスワードは変わっていません。');
  };

  return (
    <Row title="パスワード" desc="変えると、ほかの端末の記憶は解除されます。">
      {!open && (
        <div className="flex flex-wrap items-center gap-3">
          <Button className={BTN} onClick={() => { setOpen(true); setDone(false); }}>変更する</Button>
          {done && <span className="flex items-center gap-1 text-xs text-pt-ok"><IconCheck size={14} />変更しました。ほかの端末は次回から確認コードが必要です。</span>}
        </div>
      )}
      {open && (
        <div className="flex max-w-sm flex-col gap-3">
          <Field label="今のパスワード" error={errors.current} hint={`試作用の入力例: ${SAMPLE_PASSWORD}`} htmlFor="pw-current">
            <PasswordInput compact id="pw-current" name="current-password" autoComplete="current-password" value={current} onChange={setCurrent} />
          </Field>
          <Field label="新しいパスワード" hint={`12 文字以上（いま ${String(next.length)} 文字）`} error={errors.next} htmlFor="pw-next">
            <PasswordInput compact id="pw-next" name="new-password" autoComplete="new-password" value={next} onChange={setNext} />
          </Field>
          <Field label="新しいパスワード（確認）" error={errors.again} htmlFor="pw-again">
            <PasswordInput compact id="pw-again" name="confirm-password" autoComplete="new-password" value={again} onChange={setAgain} />
          </Field>
          {save.error && <Failed>{save.error}</Failed>}
          <div className="flex gap-2">
            <Button variant="primary" className={BTN} disabled={save.busy} onClick={submit}>{save.busy ? '変更中…' : '変更する'}</Button>
            <Button className={BTN} disabled={save.busy} onClick={() => { setOpen(false); setErrors({}); save.clear(); }}>やめる</Button>
          </div>
        </div>
      )}
    </Row>
  );
}

function PhoneRow() {
  const [copied, setCopied] = useState(false);
  const short = LOGIN_URL.replace('https://', '');
  const copy = () => {
    void navigator.clipboard.writeText(LOGIN_URL).catch(() => undefined);
    setCopied(true);
  };
  return (
    <Row title="スマホでも使う" desc="ホーム画面に追加すると、すぐ開けます。">
      <div className="space-y-2 text-[13px] text-pt-muted sm:hidden">
        <p><span className="font-medium text-pt-ink">iPhone（Safari）</span>: 画面下の共有ボタンを押し、「ホーム画面に追加」を選びます。</p>
        <p><span className="font-medium text-pt-ink">Android（Chrome）</span>: 右上のメニューを押し、「ホーム画面に追加」を選びます。</p>
      </div>
      <div className="hidden flex-wrap items-center gap-2 sm:flex">
        <code className="pt-num whitespace-nowrap rounded-md border border-pt-line bg-pt-surface px-2.5 py-1.5 text-xs text-pt-ink">{short}</code>
        <Button className={BTN} onClick={copy}><IconCopy size={14} />{copied ? 'コピーしました' : 'コピー'}</Button>
      </div>
      <p className="mt-2 text-xs text-pt-muted">スマホでの初回ログインには、確認コードが必要です。</p>
    </Row>
  );
}

function TermsRow() {
  const [open, setOpen] = useState(false);
  return (
    <Row title="利用規約">
      <button type="button" onClick={() => { setOpen(true); }} className="flex h-11 w-full max-w-sm items-center justify-between rounded-md border border-pt-line bg-pt-surface px-3 text-left text-pt-ink hover:bg-pt-bg sm:h-9">
        読む
        <IconChevronRight size={14} className="text-pt-faint" />
      </button>
      <Dialog open={open} title="利用規約" width="sm" onClose={() => { setOpen(false); }}>
        <ol className="list-decimal space-y-1.5 pl-4 text-[13px] leading-relaxed text-pt-ink">
          <li>応募者の情報は、採用の連絡と選考の管理にだけお使いください。</li>
          <li>メールアドレスとパスワードは、ほかの方と共有しないでください。</li>
          <li>担当者が変わったときは、弊社にご連絡ください。</li>
        </ol>
      </Dialog>
    </Row>
  );
}

function LogoutRow() {
  const { go } = usePreviewNav();
  return (
    <section className="flex items-center gap-4 border-t border-pt-line py-4 lg:hidden">
      <div className="min-w-0 flex-1">
        <h2 className="text-[13px] font-semibold text-pt-ink">ログアウト</h2>
        <p className="mt-0.5 text-xs text-pt-muted">共用の端末では、使い終わったら押してください。</p>
      </div>
      <Button className="min-w-28" onClick={() => { go(ROUTES.loggedOut); }}><IconLogout size={14} />ログアウト</Button>
    </section>
  );
}

function ColumnsLink() {
  const { go } = usePreviewNav();
  return (
    <button
      type="button"
      onClick={() => { go(ROUTES.columns); }}
      className="mb-2 flex h-[52px] w-full items-center justify-between rounded-lg border border-pt-line bg-pt-surface px-4 text-left text-[15px] font-medium text-pt-ink hover:bg-pt-bg sm:text-[13px] lg:hidden"
    >
      項目の設定
      <IconChevronRight size={16} className="text-pt-faint" />
    </button>
  );
}

function AccountBody({ account, passwordDone, failing, companyName, canEditColumns }: { account: PortalAccount; passwordDone: boolean; failing: boolean; companyName: string; canEditColumns: boolean }) {
  const [devices, setDevices] = useState<TrustedDevice[]>(() =>
    initialDevices.filter((d) => d.accountId === account.id && (!passwordDone || d.current)),
  );
  return (
    <div className="max-w-3xl">
      {companyName !== '' && <p className="mb-3 truncate text-[15px] font-semibold text-pt-ink">{companyName}</p>}
      {canEditColumns && <ColumnsLink />}
      <ProfileRow account={account} failing={failing} />
      <PasswordRow failing={failing} initialDone={passwordDone} onChanged={() => { setDevices((l) => l.filter((d) => d.current)); }} />
      <DeviceRow list={devices} setList={setDevices} failing={failing} />
      <CodeDestinationRow account={account} failing={failing} />
      <PhoneRow />
      <TermsRow />
      <Row title="連絡先" desc="ログインできないとき、担当者を追加したいとき。">
        <p className="mb-1 text-pt-ink">{SUPPORT.name}</p>
        <ContactLines subject="アカウントについて" />
      </Row>
      <LogoutRow />
    </div>
  );
}

export function AccountScreen({ passwordDone = false, failing = false }: { passwordDone?: boolean; failing?: boolean }) {
  const d = useData();
  const { go } = usePreviewNav();
  const viewerId = d.viewer.kind === 'customer' ? d.viewer.accountId : '';
  const account = d.viewer.kind === 'customer' ? d.accounts.find((a) => a.id === viewerId) : undefined;
  const companyName = currentCustomer(d)?.name ?? '';
  const canEditColumns = currentSheet(d)?.customerCanEditColumns === true;
  return (
    <AppShell section="account" title="アカウント">
      {account ? (
        <AccountBody key={account.id} account={account} passwordDone={passwordDone} failing={failing} companyName={companyName} canEditColumns={canEditColumns} />
      ) : (
        <div className="max-w-3xl py-4 text-[13px]">
          <p className="text-pt-muted">社内の人の設定は、社内の管理画面で行います。</p>
          <Button className="mt-3" onClick={() => { go(ROUTES.adminAccounts); }}>アカウントの管理へ</Button>
        </div>
      )}
    </AppShell>
  );
}
