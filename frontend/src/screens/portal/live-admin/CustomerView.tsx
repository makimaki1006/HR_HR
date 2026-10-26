// 1 社の顧客: 名前・段階・停止、登録したシート、アカウント (招待・停止)。
import { useState } from 'react';
import type { PortalAdminAccount } from '../../../generated/PortalAdminAccount';
import type { PortalAdminCustomer } from '../../../generated/PortalAdminCustomer';
import type { PortalAdminCustomerDetail } from '../../../generated/PortalAdminCustomerDetail';
import type { PortalAdminInvite } from '../../../generated/PortalAdminInvite';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { Badge, Button, Dialog, Segmented, formatShortDateTime } from '../ui/kit';
import { IconExternal, IconPlus } from '../ui/icons';
import { API, post, put } from './api';
import { ACCOUNT_STATUS_LABEL, STAGE_LABEL, STAGE_MEANING, stageOf, type Stage } from './model';
import { ErrorLine, InviteLinkDialog, Link, LoadError, Loading, Page, Section, TD, TH, Table, TextField, useGet, useNav } from './parts';
import { NotifyBadge } from './NotifySettings';

export function AccountRows({ accounts, onInvite, onToggle, busy, customerName }: {
  accounts: PortalAdminAccount[];
  onInvite: (a: PortalAdminAccount) => void;
  onToggle: (a: PortalAdminAccount) => void;
  busy: boolean;
  customerName?: (id: string) => string;
}) {
  return (
    <Table minWidth={860}>
      <thead>
        <tr>
          <th className={TH}>メールアドレス</th>
          {customerName && <th className={TH}>顧客</th>}
          <th className={TH}>状態</th>
          <th className={TH}>最後のログイン</th>
          <th className={TH}>確認コードの追加の送り先</th>
          <th className={TH + ' w-56'} />
        </tr>
      </thead>
      <tbody>
        {accounts.map((a) => (
          <tr key={a.id}>
            <td className={TD}>{a.email}</td>
            {customerName && <td className={TD}>{customerName(a.customer_id)}</td>}
            <td className={TD}><Badge tone={a.status === 'active' ? 'ok' : a.status === 'invited' ? 'info' : 'neutral'}>{ACCOUNT_STATUS_LABEL[a.status] ?? a.status}</Badge></td>
            <td className={TD + ' pt-num text-pt-muted'}>{a.last_login_at !== null ? formatShortDateTime(a.last_login_at) : <span className="text-pt-faint">まだない</span>}</td>
            <td className={TD + ' text-pt-muted'}>{a.extra_code_email ?? <span className="text-pt-faint">なし</span>}</td>
            <td className={TD + ' text-right whitespace-nowrap'}>
              {a.status !== 'disabled' && (
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => { onInvite(a); }}>
                  {a.status === 'invited' ? '招待のリンクを出し直す' : '再設定のリンクを出す'}
                </Button>
              )}
              <Button size="sm" variant={a.status === 'disabled' ? 'secondary' : 'danger'} disabled={busy} onClick={() => { onToggle(a); }}>
                {a.status === 'disabled' ? '再開' : '停止'}
              </Button>
            </td>
          </tr>
        ))}
        {accounts.length === 0 && <tr><td colSpan={customerName ? 6 : 5} className="px-3 py-8 text-center text-pt-muted">まだアカウントがありません</td></tr>}
      </tbody>
    </Table>
  );
}

/** アカウントの招待・出し直し・停止・再開。結果のリンクはダイアログで 1 回だけ出す */
export function useAccountActions(reload: () => void): {
  invite: (email: string, customerId: string) => Promise<boolean>;
  reissue: (a: PortalAdminAccount) => void;
  toggle: (a: PortalAdminAccount) => void;
  busy: boolean;
  error: string | null;
  dialog: React.ReactNode;
} {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [link, setLink] = useState<{ email: string; path: string } | null>(null);
  const [confirm, setConfirm] = useState<PortalAdminAccount | null>(null);

  const run = async <T,>(f: () => Promise<{ ok: true; data: T } | { ok: false; message: string }>): Promise<T | null> => {
    setBusy(true);
    setError(null);
    const r = await f();
    setBusy(false);
    reload();
    if (!r.ok) {
      setError(r.message);
      return null;
    }
    return r.data;
  };

  const invite = async (email: string, customerId: string): Promise<boolean> => {
    const d = await run(() => post<PortalAdminInvite>(`${API}/accounts`, { email, customer_id: customerId }));
    if (d !== null) setLink({ email: d.account.email, path: d.invite_path });
    return d !== null;
  };
  const reissue = (a: PortalAdminAccount): void => {
    void run(() => post<PortalAdminInvite>(`${API}/accounts/${encodeURIComponent(a.id)}/invite`, {})).then((d) => {
      if (d !== null) setLink({ email: d.account.email, path: d.invite_path });
    });
  };
  const toggle = (a: PortalAdminAccount): void => {
    if (a.status === 'disabled') {
      void run(() => post<PortalAdminAccount>(`${API}/accounts/${encodeURIComponent(a.id)}/enable`, {}));
    } else {
      setConfirm(a);
    }
  };
  const dialog = (
    <>
      <InviteLinkDialog email={link?.email ?? ''} path={link?.path ?? null} onClose={() => { setLink(null); }} />
      <Dialog
        open={confirm !== null}
        title="アカウントを止める"
        width="sm"
        onClose={() => { setConfirm(null); }}
        footer={
          <>
            <Button variant="ghost" onClick={() => { setConfirm(null); }}>やめる</Button>
            <Button
              variant="danger"
              disabled={busy}
              onClick={() => {
                const a = confirm;
                setConfirm(null);
                if (a) void run(() => post<PortalAdminAccount>(`${API}/accounts/${encodeURIComponent(a.id)}/disable`, {}));
              }}
            >
              止める
            </Button>
          </>
        }
      >
        <p className="text-sm">{confirm?.email} さんのアカウントを止めます。ログイン中の画面と覚えた端末もすぐ使えなくなります。</p>
      </Dialog>
    </>
  );
  return { invite, reissue, toggle, busy, error, dialog };
}

function CustomerSettings({ customer, canAdvance, onSaved }: { customer: PortalAdminCustomer; canAdvance: boolean; onSaved: () => void }) {
  const [name, setName] = useState(customer.name);
  const [hubspot, setHubspot] = useState(customer.hubspot_company_id ?? '');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const stage = stageOf(customer.stage);

  const save = async (body: Record<string, unknown>): Promise<void> => {
    setBusy(true);
    setError(null);
    const r = await put<PortalAdminCustomer>(`${API}/customers/${encodeURIComponent(customer.id)}`, body);
    setBusy(false);
    if (!r.ok) setError(r.message);
    onSaved();
  };
  const dirty = name.trim() !== customer.name || hubspot.trim() !== (customer.hubspot_company_id ?? '');

  return (
    <div className="grid gap-4 rounded-lg border border-pt-line bg-pt-surface p-4 md:grid-cols-2">
      <div className="flex flex-col gap-3">
        <TextField label="顧客名" value={name} maxLength={100} onChange={(e) => { setName(e.target.value); }} />
        <TextField label="HubSpot の会社 ID（任意）" value={hubspot} inputMode="numeric" maxLength={20} onChange={(e) => { setHubspot(e.target.value); }} />
        <div>
          <Button size="sm" variant="primary" disabled={!dirty || busy} onClick={() => { void save({ name, hubspot_company_id: hubspot }); }}>名前と ID を保存</Button>
        </div>
      </div>
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <span className="text-sm font-medium">使い方の段階</span>
          <Segmented<Stage>
            size="sm"
            value={stage}
            onChange={(v) => { if (v !== stage) void save({ stage: v }); }}
            options={(['sheet_only', 'both', 'portal_only'] as const).map((v) => ({ value: v, label: STAGE_LABEL[v] }))}
          />
          <p className="text-xs text-pt-muted">{STAGE_MEANING[stage]}</p>
          {!canAdvance && stage === 'sheet_only' && <p className="text-xs text-pt-warn">「両方」「画面だけ」にするには、先にシートを登録して、顧客に見せる項目を設定します。</p>}
        </div>
        <div className="flex items-center gap-3">
          <span className="text-sm font-medium">状態</span>
          <Badge tone={customer.disabled ? 'neutral' : 'ok'}>{customer.disabled ? '停止中' : '利用中'}</Badge>
          <Button size="sm" variant={customer.disabled ? 'secondary' : 'danger'} disabled={busy} onClick={() => { void save({ disabled: !customer.disabled }); }}>
            {customer.disabled ? '再開する' : 'この顧客を止める'}
          </Button>
        </div>
        <p className="text-xs text-pt-muted">止めると、この顧客のアカウントは全員すぐログインできなくなります。</p>
        <ErrorLine message={error} />
      </div>
    </div>
  );
}

export function CustomerView({ id, status }: { id: string; status: PortalAdminStatus | null }) {
  const go = useNav();
  const [load, reload] = useGet<PortalAdminCustomerDetail>(`${API}/customers/${encodeURIComponent(id)}`);
  const actions = useAccountActions(reload);
  const [email, setEmail] = useState('');
  const [inviting, setInviting] = useState(false);

  if (load.state !== 'ok') {
    return (
      <Page current="customers" title="顧客" status={status} back={{ label: '顧客の一覧', to: { view: 'customers' } }}>
        {load.state === 'loading' ? <Loading /> : <LoadError message={load.message} />}
      </Page>
    );
  }
  const { customer, sheets, accounts } = load.data;
  const canAdvance = sheets.some((s) => !s.disabled && s.visible_columns > 0);

  return (
    <Page
      current="customers"
      title={customer.name}
      badge={customer.disabled ? <Badge tone="neutral">停止中</Badge> : undefined}
      status={status}
      back={{ label: '顧客の一覧', to: { view: 'customers' } }}
    >
      <CustomerSettings key={customer.updated_at} customer={customer} canAdvance={canAdvance} onSaved={reload} />

      <Section
        title="シート"
        actions={<Button size="sm" onClick={() => { go({ view: 'sheet-add', customer: customer.id }); }}><IconPlus size={14} />シートを登録</Button>}
      >
        <Table minWidth={760}>
          <thead>
            <tr>
              <th className={TH}>顧客に見せる名前</th>
              <th className={TH}>タブ</th>
              <th className={TH + ' text-right'}>見出しの行</th>
              <th className={TH + ' text-right'}>見せる項目</th>
              <th className={TH}>状態</th>
              <th className={TH}>応募の通知</th>
              <th className={TH + ' w-24'} />
            </tr>
          </thead>
          <tbody>
            {sheets.map((s) => (
              <tr key={s.id}>
                <td className={TD}><Link to={{ view: 'sheet', id: s.id }} className="font-medium text-pt-ink hover:underline">{s.label}</Link></td>
                <td className={TD + ' text-pt-muted'}>{s.tab_title}</td>
                <td className={TD + ' pt-num text-right'}>{String(s.header_row)}</td>
                <td className={TD + ' pt-num text-right'}>{s.visible_columns === 0 ? <span className="text-pt-warn">まだ設定していない</span> : String(s.visible_columns)}</td>
                <td className={TD}>{s.disabled ? <Badge tone="neutral">停止中</Badge> : <Badge tone="ok">使う</Badge>}</td>
                <td className={TD}><NotifyBadge notify={s.notify} /></td>
                <td className={TD + ' text-right'}>
                  <a href={s.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 whitespace-nowrap text-xs text-pt-primary hover:underline">シートを開く<IconExternal size={12} /></a>
                </td>
              </tr>
            ))}
            {sheets.length === 0 && <tr><td colSpan={7} className="px-3 py-8 text-center text-pt-muted">まだシートがありません。「シートを登録」から追加します。</td></tr>}
          </tbody>
        </Table>
      </Section>

      <Section
        title="アカウント"
        actions={<Button size="sm" onClick={() => { setInviting(true); }}><IconPlus size={14} />招待する</Button>}
      >
        <AccountRows accounts={accounts} busy={actions.busy} onInvite={actions.reissue} onToggle={actions.toggle} />
        <ErrorLine message={actions.error} />
      </Section>

      <Dialog
        open={inviting}
        title="アカウントを招待"
        width="sm"
        onClose={() => { setInviting(false); }}
        footer={
          <>
            <Button variant="ghost" onClick={() => { setInviting(false); }}>やめる</Button>
            <Button
              variant="primary"
              disabled={email.trim() === '' || actions.busy}
              onClick={() => {
                void actions.invite(email, customer.id).then((ok) => {
                  if (ok) {
                    setInviting(false);
                    setEmail('');
                  }
                });
              }}
            >
              招待のリンクを出す
            </Button>
          </>
        }
      >
        <TextField label="メールアドレス" hint={`${customer.name} の担当者です。1 人 1 アカウント。リンクは本人に送ります（自動では送りません）。`} autoFocus type="email" value={email} maxLength={254} onChange={(e) => { setEmail(e.target.value); }} />
        <ErrorLine message={actions.error} />
      </Dialog>
      {actions.dialog}
    </Page>
  );
}
