// /app/portal-admin: 顧客向け応募者画面の社内の管理 (計画 10 章 PR1)。管理者だけ (サーバーが 403 を返す)。
// 顧客・シートの登録、試しに読む、項目の設定、アカウントの招待・停止、操作の記録。
// 画面の中は ?view= で切り替える (戻る・新しいタブで同じ画面を開ける)。
import { useCallback, useEffect, useState } from 'react';
import type { PortalAdminAccounts } from '../../../generated/PortalAdminAccounts';
import type { PortalAdminCustomers } from '../../../generated/PortalAdminCustomers';
import type { PortalAdminLogs } from '../../../generated/PortalAdminLogs';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { formatShortDateTime } from '../ui/kit';
import { API } from './api';
import { AccountRows, useAccountActions } from './CustomerView';
import { CustomerView } from './CustomerView';
import { CustomersView } from './CustomersView';
import { MailView } from './MailView';
import { LOG_KIND_LABEL, SOURCE_LABEL, parseRoute, routeHref, type Route } from './model';
import { ErrorLine, LoadError, Loading, NavContext, Page, TD, TH, Table, useGet, useStatus } from './parts';
import { SheetAddView } from './SheetAddView';
import { SheetView } from './SheetView';

function AccountsView({ status }: { status: PortalAdminStatus | null }) {
  const [load, reload] = useGet<PortalAdminAccounts>(`${API}/accounts`);
  const [customers] = useGet<PortalAdminCustomers>(`${API}/customers`);
  const actions = useAccountActions(reload);
  const names = new Map(customers.state === 'ok' ? customers.data.customers.map((c) => [c.id, c.name]) : []);
  return (
    <Page current="accounts" title="アカウント" status={status}>
      <p className="mb-3 text-xs text-pt-muted">招待は顧客の画面から行います（顧客とシート → 顧客を選ぶ → 招待する）。</p>
      {load.state === 'loading' && <Loading />}
      {load.state === 'error' && <LoadError message={load.message} />}
      {load.state === 'ok' && (
        <AccountRows
          accounts={load.data.accounts}
          busy={actions.busy}
          onInvite={actions.reissue}
          onToggle={actions.toggle}
          customerName={(id) => names.get(id) ?? '（登録の無い顧客）'}
        />
      )}
      <ErrorLine message={actions.error} />
      {actions.dialog}
    </Page>
  );
}

export function ActivityRows({ logs, customerName }: { logs: PortalAdminLogs; customerName: (id: string) => string }) {
  return (
    <Table minWidth={900}>
      <thead>
        <tr>
          <th className={TH}>日時</th>
          <th className={TH}>誰が</th>
          <th className={TH}>どこから</th>
          <th className={TH}>操作</th>
          <th className={TH}>顧客</th>
          <th className={TH}>内容</th>
        </tr>
      </thead>
      <tbody>
        {logs.entries.map((e, i) => (
          <tr key={`${e.at}-${String(i)}`}>
            <td className={TD + ' pt-num whitespace-nowrap text-pt-muted'}>{formatShortDateTime(e.at)}</td>
            <td className={TD}>{e.source === 'portal' ? '顧客の担当者' : e.actor}</td>
            <td className={TD + ' text-pt-muted'}>{SOURCE_LABEL[e.source] ?? e.source}</td>
            <td className={TD}>{LOG_KIND_LABEL[e.kind] ?? e.kind}</td>
            <td className={TD}>{e.customer_id === '' ? '' : customerName(e.customer_id)}</td>
            <td className={TD + ' text-pt-muted'}>
              {e.column !== '' ? `${e.column}: ${e.before === '' ? '（空）' : e.before} → ${e.after === '' ? '（空）' : e.after}` : e.detail}
            </td>
          </tr>
        ))}
        {logs.entries.length === 0 && <tr><td colSpan={6} className="px-3 py-8 text-center text-pt-muted">まだ記録がありません</td></tr>}
      </tbody>
    </Table>
  );
}

function ActivityView({ status }: { status: PortalAdminStatus | null }) {
  const [load] = useGet<PortalAdminLogs>(`${API}/activity`);
  const [customers] = useGet<PortalAdminCustomers>(`${API}/customers`);
  const names = new Map(customers.state === 'ok' ? customers.data.customers.map((c) => [c.id, c.name]) : []);
  return (
    <Page current="activity" title="操作の記録" status={status}>
      <p className="mb-3 text-xs text-pt-muted">アプリが起動してからの直近の記録です。それより前の記録は社内用シートの「操作の記録」タブで見られます。</p>
      {load.state === 'loading' && <Loading />}
      {load.state === 'error' && <LoadError message={load.message} />}
      {load.state === 'ok' && <ActivityRows logs={load.data} customerName={(id) => names.get(id) ?? id} />}
    </Page>
  );
}

export function PortalAdminApp() {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.search));
  const [status, reloadStatus] = useStatus();
  useEffect(() => {
    const onPop = (): void => { setRoute(parseRoute(window.location.search)); };
    window.addEventListener('popstate', onPop);
    return () => { window.removeEventListener('popstate', onPop); };
  }, []);
  const go = useCallback((r: Route) => {
    window.history.pushState(null, '', routeHref(r));
    window.scrollTo(0, 0);
    setRoute(r);
    reloadStatus();
  }, [reloadStatus]);
  // 保存待ちが残っている間は、帯を 10 秒ごとに読み直す
  useEffect(() => {
    if (status === null || status.pending === 0) return undefined;
    const t = window.setInterval(reloadStatus, 10_000);
    return () => { window.clearInterval(t); };
  }, [status, reloadStatus]);

  const key = routeHref(route);
  return (
    <NavContext.Provider value={go}>
      {route.view === 'customers' && <CustomersView key={key} status={status} />}
      {route.view === 'customer' && <CustomerView key={key} id={route.id} status={status} />}
      {route.view === 'sheet-add' && <SheetAddView key={key} customerId={route.customer} status={status} />}
      {route.view === 'sheet' && <SheetView key={key} id={route.id} status={status} />}
      {route.view === 'accounts' && <AccountsView key={key} status={status} />}
      {route.view === 'activity' && <ActivityView key={key} status={status} />}
      {route.view === 'mail' && <MailView key={key} status={status} />}
    </NavContext.Provider>
  );
}
