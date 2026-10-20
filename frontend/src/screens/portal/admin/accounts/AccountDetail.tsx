import { useState, type ReactNode } from 'react';
import { useData } from '../../data';
import { usePreviewNav } from '../../preview';
import { Button, Dialog, formatDateTimeSmart, formatShortDateTime } from '../../ui/kit';
import { AdminPage, MoreMenu, type MenuItem } from '../customers/shared';
import { entrySentence } from './activityView';
import {
  customerName,
  isInviteExpired,
  issueResetLink,
  reissueInvite,
  removeDevices,
  setActivityAccount,
  setDisabled,
  unlock,
  useAccountsState,
  type Issued,
} from './store';
import { AccountStatusBadge, CopyButton } from './shared';

type Confirm = null | 'disable' | 'devices' | 'reset';

function Block({ title, right, children }: { title: string; right?: ReactNode; children: ReactNode }) {
  return (
    <section>
      <div className="mb-1 flex items-center gap-2 border-b border-pt-line pb-1.5">
        <h2 className="text-[13px] font-semibold">{title}</h2>
        {right && <div className="ml-auto">{right}</div>}
      </div>
      {children}
    </section>
  );
}

export function AccountDetail() {
  const nav = usePreviewNav();
  const d = useData();
  const { accounts, devices, selectedId } = useAccountsState();
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [confirmDevice, setConfirmDevice] = useState<string | null>(null);
  const [issued, setIssued] = useState<(Issued & { what: string }) | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const account = accounts.find((a) => a.id === selectedId) ?? accounts[0];
  if (!account) return null;
  const myDevices = devices.filter((x) => x.accountId === account.id);
  const recent = d.activity.filter((e) => e.accountEmail === account.email).slice(0, 8);
  const done = (m: string): void => { setMessage(m); setConfirm(null); setConfirmDevice(null); };
  const invited = account.status === 'invited';
  const expired = isInviteExpired(account);
  const issueInvite = (): void => {
    const r = reissueInvite(account.id);
    if (r) setIssued({ ...r, what: '招待リンク' });
    done('招待リンクを出し直した。');
  };
  const issueReset = (): void => {
    const r = issueResetLink(account.id);
    if (r) setIssued({ ...r, what: '再設定のリンク' });
    done('パスワード再設定のリンクを出した。');
  };
  const dt = 'text-pt-muted';

  const customerStopped = d.customers.find((c) => c.id === account.customerId)?.disabled === true;
  const menu: MenuItem[] = [
    ...(account.status === 'disabled' ? [{ label: 'アカウントを再開', onSelect: () => { setDisabled(account.id, false); done('アカウントを再開した。'); } }] : [{ label: 'アカウントを停止', danger: true, onSelect: () => { setConfirm('disable'); } }]),
    { label: '覚えている端末をすべて解除', danger: true, disabled: myDevices.length === 0, onSelect: () => { setConfirm('devices'); } },
  ];

  return (
    <AdminPage
      title={account.displayName || account.email}
      titleBadge={<AccountStatusBadge account={account} />}
      current="accounts"
      back={{ label: 'アカウント', to: 'admin-accounts' }}
      actions={
        <>
          {account.status === 'locked' && <Button size="sm" onClick={() => { unlock(account.id); done('ロックを解除した。'); }}>今すぐ解除</Button>}
          {invited ? (
            <Button size="sm" onClick={issueInvite}>招待リンクを出し直す</Button>
          ) : (
            <Button size="sm" disabled={account.status === 'disabled'} onClick={() => { setConfirm('reset'); }}>パスワード再設定のリンクを出す</Button>
          )}
          <MoreMenu items={menu} />
        </>
      }
    >
      {message && <p className="mb-3 text-[13px] text-pt-ok">{message}</p>}
      {customerStopped && <p className="mb-3 text-[13px] text-pt-muted">この顧客は停止中のため、ログインできない。</p>}
      {invited && <p className="mb-3 text-xs text-pt-muted">招待リンクを出し直すと、前のリンクは使えなくなる。</p>}
      {account.status === 'disabled' && <p className="mb-3 text-xs text-pt-muted">再設定のリンクは、再開してから出せる。</p>}
      {issued && (
        <div className="mb-4 rounded-md border border-pt-line bg-pt-sunken p-3 text-[13px]">
          <p className="pt-num text-xs text-pt-muted">{issued.what}：{formatShortDateTime(issued.expiresAt)} まで有効、1 回だけ使える。本人に伝える。</p>
          <div className="mt-1.5 flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate text-xs" title={issued.link}>{issued.link}</code>
            <CopyButton text={issued.link} label={issued.what + 'をコピー'} />
          </div>
        </div>
      )}
      <div className="grid gap-8 lg:grid-cols-2">
        <div className="space-y-6">
          <Block title="登録の内容">
            <dl className="grid grid-cols-[13rem_1fr] text-[13px] [&>*]:border-b [&>*]:border-pt-line [&>*]:py-2 [&>*:nth-last-child(-n+2)]:border-b-0">
              <dt className={dt}>メールアドレス</dt><dd>{account.email}</dd>
              <dt className={dt}>顧客</dt><dd>{customerName(account.customerId)}</dd>
              <dt className={dt}>最後のログイン</dt><dd className="pt-num">{account.lastLoginAt ? formatShortDateTime(account.lastLoginAt) : 'まだない'}</dd>
              <dt className={dt}>確認コードの追加の送り先</dt><dd>{account.extraCodeEmail ?? <span className="text-pt-faint">なし</span>}</dd>
              {invited && account.inviteExpiresAt && (
                <>
                  <dt className={dt}>招待の期限</dt>
                  <dd className="pt-num">{formatShortDateTime(account.inviteExpiresAt)}{expired && <span className="ml-2 text-pt-danger">期限が切れている</span>}</dd>
                </>
              )}
              {account.status === 'locked' && (
                <>
                  <dt className={dt}>ロック</dt>
                  <dd className="pt-num">{account.lockedUntil ? formatDateTimeSmart(account.lockedUntil) + ' に自動で解除' : '時間がたつと自動で解除'}</dd>
                </>
              )}
            </dl>
          </Block>

          <Block title={`覚えている端末（${String(myDevices.length)} 台）`}>
            {myDevices.length === 0 ? (
              <p className="py-2 text-[13px] text-pt-muted">覚えている端末はない。</p>
            ) : (
              <ul>
                {myDevices.map((x) => (
                  <li key={x.id} className="flex flex-wrap items-center gap-3 border-b border-pt-line py-2 text-[13px] last:border-0">
                    <div className="min-w-0 flex-1">
                      <p className="font-medium">{x.label}</p>
                      <p className="pt-num text-xs text-pt-muted">最後に使った {formatShortDateTime(x.lastUsedAt)} ／ {formatDateTimeSmart(x.expiresAt)} まで</p>
                    </div>
                    {confirmDevice === x.id ? (
                      <span className="flex items-center gap-1.5">
                        <Button size="sm" variant="danger" onClick={() => { removeDevices(account.id, x.id); done('端末を 1 台解除した。'); }}>解除する</Button>
                        <Button size="sm" variant="ghost" onClick={() => { setConfirmDevice(null); }}>やめる</Button>
                      </span>
                    ) : (
                      <Button size="sm" onClick={() => { setConfirmDevice(x.id); }}>解除</Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Block>
        </div>

        <div>
          <Block title="最近の記録" right={<button type="button" onClick={() => { setActivityAccount(account.id); nav.go('admin-activity'); }} className="text-xs text-pt-primary hover:underline">すべての記録</button>}>
            {recent.length === 0 ? (
              <p className="py-2 text-[13px] text-pt-muted">記録はまだない。</p>
            ) : (
              <ul>
                {recent.map((e) => (
                  <li key={e.id} className="flex gap-3 border-b border-pt-line py-2 text-[13px] last:border-b-0">
                    <span className="pt-num w-24 shrink-0 text-xs leading-5 text-pt-muted">{formatShortDateTime(e.at)}</span>
                    <span className="min-w-0 flex-1 [overflow-wrap:anywhere]">{entrySentence(d, e)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Block>
        </div>
      </div>

      <Dialog
        open={confirm === 'reset'}
        width="sm"
        title="パスワード再設定のリンクを出す"
        onClose={() => { setConfirm(null); }}
        footer={<><Button variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button><Button variant="primary" onClick={issueReset}>リンクを出す</Button></>}
      >
        <p className="text-[13px]">新しいリンクを出すと、前のリンクは使えなくなる。今のパスワードは、リンクで決め直すまで使える。</p>
      </Dialog>
      <Dialog
        open={confirm === 'disable'}
        width="sm"
        title="このアカウントを停止"
        onClose={() => { setConfirm(null); }}
        footer={<><Button variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button><Button variant="danger" onClick={() => { setDisabled(account.id, true); done('アカウントを停止した。'); }}>停止する</Button></>}
      >
        <p className="text-[13px]">すぐにログインできなくなる。開いている画面も次の操作で終わる。再開はいつでもできる。</p>
      </Dialog>
      <Dialog
        open={confirm === 'devices'}
        width="sm"
        title={`覚えている端末 ${String(myDevices.length)} 台をすべて解除`}
        onClose={() => { setConfirm(null); }}
        footer={<><Button variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button><Button variant="danger" onClick={() => { removeDevices(account.id); done('覚えている端末を解除した。'); }}>解除する</Button></>}
      >
        <p className="text-[13px]">次にログインするとき、どの端末でも確認コードが必要になる。元に戻せない。</p>
      </Dialog>
    </AdminPage>
  );
}
