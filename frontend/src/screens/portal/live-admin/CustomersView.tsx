// 顧客の一覧と追加。
import { useState } from 'react';
import type { PortalAdminCustomer } from '../../../generated/PortalAdminCustomer';
import type { PortalAdminCustomers } from '../../../generated/PortalAdminCustomers';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { Badge, Button, Dialog, formatShortDateTime } from '../ui/kit';
import { IconPlus } from '../ui/icons';
import { API, post } from './api';
import { STAGE_LABEL, stageOf } from './model';
import { ErrorLine, Link, LoadError, Loading, Page, TD, TH, Table, TextField, useGet, useNav } from './parts';

export function CustomerRows({ customers }: { customers: PortalAdminCustomer[] }) {
  return (
    <Table minWidth={820}>
      <thead>
        <tr>
          <th className={TH}>顧客</th>
          <th className={TH}>段階</th>
          <th className={TH + ' text-right'}>シート</th>
          <th className={TH + ' text-right'}>アカウント</th>
          <th className={TH}>最後のログイン</th>
        </tr>
      </thead>
      <tbody>
        {customers.map((c) => (
          <tr key={c.id}>
            <td className={TD}>
              <Link to={{ view: 'customer', id: c.id }} className="font-medium text-pt-ink hover:underline">{c.name}</Link>
              {c.disabled && <span className="ml-2"><Badge tone="neutral">停止中</Badge></span>}
            </td>
            <td className={TD}><Badge tone={c.stage === 'sheet_only' ? 'neutral' : 'ok'}>{STAGE_LABEL[stageOf(c.stage)]}</Badge></td>
            <td className={TD + ' pt-num text-right'}>{String(c.sheet_count)} 枚</td>
            <td className={TD + ' pt-num text-right'}>
              {String(c.account_count)}
              {c.active_account_count !== c.account_count && <span className="ml-1 text-xs text-pt-faint">（利用中 {String(c.active_account_count)}）</span>}
            </td>
            <td className={TD + ' pt-num text-pt-muted'}>{c.last_login_at !== null ? formatShortDateTime(c.last_login_at) : <span className="text-pt-faint">まだない</span>}</td>
          </tr>
        ))}
        {customers.length === 0 && (
          <tr><td colSpan={5} className="px-3 py-10 text-center text-pt-muted">まだ顧客がいません。「顧客を追加」から登録します。</td></tr>
        )}
      </tbody>
    </Table>
  );
}

export function CustomersView({ status }: { status: PortalAdminStatus | null }) {
  const go = useNav();
  const [load] = useGet<PortalAdminCustomers>(`${API}/customers`);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [hubspot, setHubspot] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const create = async (): Promise<void> => {
    if (name.trim() === '' || busy) return;
    setBusy(true);
    setError(null);
    const r = await post<PortalAdminCustomer>(`${API}/customers`, { name, hubspot_company_id: hubspot });
    setBusy(false);
    if (!r.ok) {
      setError(r.message);
      return;
    }
    setAdding(false);
    go({ view: 'customer', id: r.data.id });
  };

  return (
    <Page
      current="customers"
      title="顧客とシート"
      status={status}
      actions={<Button variant="primary" size="sm" onClick={() => { setAdding(true); }}><IconPlus size={14} />顧客を追加</Button>}
    >
      {load.state === 'loading' && <Loading />}
      {load.state === 'error' && <LoadError message={load.message} />}
      {load.state === 'ok' && <CustomerRows customers={load.data.customers} />}
      <Dialog
        open={adding}
        title="顧客を追加"
        width="sm"
        onClose={() => { setAdding(false); }}
        footer={<><Button variant="ghost" onClick={() => { setAdding(false); }}>やめる</Button><Button variant="primary" disabled={name.trim() === '' || busy} onClick={() => { void create(); }}>追加</Button></>}
      >
        <div className="flex flex-col gap-3">
          <TextField label="顧客名" hint="社内で見分ける名前です。顧客の画面には出しません。" autoFocus value={name} maxLength={100} onChange={(e) => { setName(e.target.value); }} />
          <TextField label="HubSpot の会社 ID（任意）" hint="数字だけ。後から入れられます。" value={hubspot} inputMode="numeric" maxLength={20} onChange={(e) => { setHubspot(e.target.value); }} />
          <ErrorLine message={error} />
        </div>
      </Dialog>
    </Page>
  );
}
