import { useState } from 'react';
import { usePreviewNav } from '../../preview';
import { Button, Card, Notice, formatDateTimeSmart, formatShortDateTime } from '../../ui/kit';
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
import { AccountStatusBadge, AccountsAdminLayout, CopyButton, KindBadge, StaffMark } from './shared';

type Confirm = null | 'disable' | 'devices' | 'reset';

export function AccountDetail() {
  const nav = usePreviewNav();
  const { accounts, devices, activity, selectedId } = useAccountsState();
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [confirmDevice, setConfirmDevice] = useState<string | null>(null);
  const [issued, setIssued] = useState<(Issued & { what: string }) | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const account = accounts.find((a) => a.id === selectedId) ?? accounts[0];
  if (!account) return null;
  const myDevices = devices.filter((d) => d.accountId === account.id);
  const log = activity.filter((e) => e.target === account.email).slice(0, 8);
  const done = (m: string) => { setMessage(m); setConfirm(null); setConfirmDevice(null); };
  const invited = account.status === 'invited';
  const expired = isInviteExpired(account);
  const issueInvite = () => {
    const r = reissueInvite(account.id);
    if (r) setIssued({ ...r, what: '招待リンク' });
    done('招待リンクを出し直しました。');
  };
  const issueReset = () => {
    const r = issueResetLink(account.id);
    if (r) setIssued({ ...r, what: '再設定のリンク' });
    done('パスワード再設定のリンクを出しました。');
  };
  const openLog = () => {
    setActivityAccount(account.id);
    nav.go('admin-activity');
  };

  return (
    <AccountsAdminLayout title="アカウントの詳細">
      <button type="button" onClick={() => { nav.go('admin-accounts'); }} className="mb-3 text-sm text-pt-primary hover:underline">アカウントの一覧に戻る</button>
      {message && <div className="mb-4"><Notice tone="ok">{message}</Notice></div>}
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
        <div className="space-y-4">
          <Card className="p-5">
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-lg font-semibold">{account.displayName || account.email}</p>
              <AccountStatusBadge account={account} />
              {account.status === 'locked' && <span className="text-sm text-pt-muted">{account.lockedUntil ? formatDateTimeSmart(account.lockedUntil) + ' に自動で解除されます' : '時間がたつと自動で解除されます'}</span>}
            </div>
            <dl className="mt-3 grid grid-cols-[11rem_1fr] gap-y-2 text-sm">
              <dt className="text-pt-muted">メールアドレス</dt><dd>{account.email}</dd>
              <dt className="text-pt-muted">顧客</dt><dd>{customerName(account.customerId)}</dd>
              <dt className="text-pt-muted">最後のログイン</dt><dd>{account.lastLoginAt ? formatShortDateTime(account.lastLoginAt) : 'まだありません'}</dd>
              <dt className="text-pt-muted">確認コードの追加の送り先</dt><dd>{account.extraCodeEmail ?? <span className="text-pt-faint">なし</span>}</dd>
              {invited && account.inviteExpiresAt && (
                <>
                  <dt className="text-pt-muted">招待の期限</dt>
                  <dd>{formatShortDateTime(account.inviteExpiresAt)}{expired && <span className="ml-2 text-pt-warn">期限が切れています</span>}</dd>
                </>
              )}
            </dl>
          </Card>

          <Card className="space-y-3 p-5">
            <p className="text-sm font-medium">操作</p>
            <div className="flex flex-wrap items-center gap-2">
              {invited ? (
                <Button onClick={issueInvite}>招待リンクを出し直す</Button>
              ) : (
                <Button disabled={account.status === 'disabled'} onClick={() => { setConfirm('reset'); }}>パスワード再設定のリンクを出す</Button>
              )}
              {account.status === 'locked' && <Button onClick={() => { unlock(account.id); done('ロックを解除しました。'); }}>今すぐ解除</Button>}
              {account.status === 'disabled'
                ? <Button onClick={() => { setDisabled(account.id, false); done('アカウントを再開しました。'); }}>再開する</Button>
                : <Button variant="danger" onClick={() => { setConfirm('disable'); }}>停止する</Button>}
              <Button variant="danger" disabled={myDevices.length === 0} onClick={() => { setConfirm('devices'); }}>覚えている端末をすべて解除</Button>
            </div>
            {invited && <p className="text-xs text-pt-muted">出し直すと、前の招待リンクは使えなくなります。</p>}
            {account.status === 'disabled' && <p className="text-xs text-pt-muted">パスワード再設定のリンクは、再開してから出せます。</p>}
            {issued && (
              <div className="rounded-md bg-pt-bg p-3 text-sm">
                <p className="text-pt-muted">{formatShortDateTime(issued.expiresAt)} まで有効、1 回だけ使えます。本人に伝えてください。</p>
                <div className="mt-1 flex items-center gap-2">
                  <code className="min-w-0 flex-1 truncate" title={issued.link}>{issued.link}</code>
                  <CopyButton text={issued.link} label={issued.what + 'をコピー'} />
                </div>
              </div>
            )}
            {confirm === 'reset' && (
              <Notice
                tone="warn"
                title="パスワード再設定のリンクを出します"
                action={
                  <span className="flex gap-2">
                    <Button size="sm" variant="primary" onClick={issueReset}>リンクを出す</Button>
                    <Button size="sm" variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button>
                  </span>
                }
              >
                新しいリンクを出すと、前に出したリンクは使えなくなります。今のパスワードは、リンクで決め直すまで使えます。
              </Notice>
            )}
            {confirm === 'disable' && (
              <Notice
                tone="danger"
                title="このアカウントを停止します"
                action={
                  <span className="flex gap-2">
                    <Button size="sm" variant="danger" onClick={() => { setDisabled(account.id, true); done('アカウントを停止しました。'); }}>停止する</Button>
                    <Button size="sm" variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button>
                  </span>
                }
              >
                停止するとすぐにログインできなくなり、画面を開いている場合も次の操作で終わります。再開はいつでもできます。
              </Notice>
            )}
            {confirm === 'devices' && (
              <Notice
                tone="danger"
                title={'覚えている端末 ' + String(myDevices.length) + ' 台をすべて解除します'}
                action={
                  <span className="flex gap-2">
                    <Button size="sm" variant="danger" onClick={() => { removeDevices(account.id); done('覚えている端末を解除しました。'); }}>解除する</Button>
                    <Button size="sm" variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button>
                  </span>
                }
              >
                次にログインするとき、どの端末でも確認コードが必要になります。元に戻せません。
              </Notice>
            )}
          </Card>
        </div>

        <div className="space-y-4">
          <Card>
            <p className="border-b border-pt-line px-5 py-3 text-sm font-medium">覚えている端末（{String(myDevices.length)} 台）</p>
            {myDevices.length === 0 ? (
              <p className="px-5 py-4 text-sm text-pt-muted">覚えている端末はありません。</p>
            ) : (
              <ul>
                {myDevices.map((d) => (
                  <li key={d.id} className="flex flex-wrap items-center gap-3 border-b border-pt-line px-5 py-3 text-sm last:border-0">
                    <div className="min-w-0 flex-1">
                      <p className="font-medium">{d.label}</p>
                      <p className="text-xs text-pt-muted">最後に使った日時 {formatShortDateTime(d.lastUsedAt)} ／ {formatDateTimeSmart(d.expiresAt)} まで覚えています</p>
                    </div>
                    {confirmDevice === d.id ? (
                      <span className="flex items-center gap-2">
                        <span className="text-xs text-pt-danger">この端末は次回、確認コードが必要になります。</span>
                        <Button size="sm" variant="danger" onClick={() => { removeDevices(account.id, d.id); done('端末を 1 台解除しました。'); }}>解除する</Button>
                        <Button size="sm" variant="ghost" onClick={() => { setConfirmDevice(null); }}>やめる</Button>
                      </span>
                    ) : (
                      <Button size="sm" onClick={() => { setConfirmDevice(d.id); }}>解除</Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Card>
          <Card>
            <div className="flex items-center justify-between border-b border-pt-line px-5 py-3">
              <p className="text-sm font-medium">最近の記録</p>
              <button type="button" onClick={openLog} className="text-sm text-pt-primary hover:underline">このアカウントのすべての記録</button>
            </div>
            {log.length === 0 ? (
              <p className="px-5 py-4 text-sm text-pt-muted">記録はまだありません。</p>
            ) : (
              <ul>
                {log.map((e) => (
                  <li key={e.id} className="flex gap-3 border-b border-pt-line px-5 py-2.5 text-sm last:border-0">
                    <span className="w-24 shrink-0 text-pt-muted">{formatShortDateTime(e.at)}</span>
                    <div className="min-w-0 flex-1">
                      <KindBadge kind={e.kind} /> <span className="ml-1">{e.detail}</span>
                      {e.internal && <p className="mt-0.5 text-xs text-pt-muted">{e.actor}<StaffMark /></p>}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </div>
      </div>
    </AccountsAdminLayout>
  );
}
