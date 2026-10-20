import { useEffect, useMemo, useState } from 'react';
import { NOW } from '../../fixtures';
import { getData, log, useData } from '../../data';
import type { DataState } from '../../data';
import type { ActivityEntry, ActivityKind } from '../../types';
import { KIND_DOT, KIND_LABEL, columnLabel, kindLabel } from '../../activityText';
import { IconDownload } from '../../ui/icons';
import { Button, Segmented, Select, cx, formatTime } from '../../ui/kit';
import { AdminPage } from '../customers/shared';
import { actorOf, applicantOf, changeText, personOf } from './activityView';
import { setActivityAccount, setActivityCustomer, useAccountsState } from './store';

type Period = 'today' | 'week' | 'month' | 'all';

const dayKey = (iso: string): string => new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Tokyo' }).format(new Date(iso));

function dayLabel(key: string): string {
  if (key === dayKey(NOW)) return '今日';
  if (key === dayKey(new Date(new Date(NOW).getTime() - 86_400_000).toISOString())) return '昨日';
  const [, m, d] = key.split('-');
  return `${String(Number(m))}/${String(Number(d))}`;
}

function inPeriod(iso: string, p: Period): boolean {
  if (p === 'all') return true;
  if (p === 'today') return dayKey(iso) === dayKey(NOW);
  return new Date(NOW).getTime() - new Date(iso).getTime() <= (p === 'week' ? 7 : 30) * 86_400_000;
}

/** その人の記録か。自分が操作した記録と、その人に対する社内の操作。 */
const involves = (e: ActivityEntry, name: string): boolean => actorOf(e) === name || (e.accountEmail !== '' && personOf(e.accountEmail) === name);

const GRID_BASE = 'grid items-center gap-3 px-3';
const COLS_ALL = 'grid-cols-[88px_120px_170px_100px_120px_minmax(0,1fr)_120px]';
const COLS_ONE = 'grid-cols-[88px_120px_100px_120px_minmax(0,1fr)_120px]';

function KindMark({ e }: { e: ActivityEntry }) {
  const kind = e.kind;
  if (kind === 'save_conflict' || kind === 'login_failed') {
    return <span className={cx('inline-flex w-fit items-center rounded px-1.5 py-0.5 text-xs font-medium whitespace-nowrap', kind === 'save_conflict' ? 'bg-pt-warn-soft text-pt-warn' : 'bg-pt-danger-soft text-pt-danger')}>{kindLabel(e)}</span>;
  }
  return <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-muted"><span className={cx('h-1.5 w-1.5 rounded-full', KIND_DOT[kind])} />{kindLabel(e)}</span>;
}

const csvCell = (v: string): string => `"${v.replace(/"/g, '""')}"`;

function exportCsv(d: DataState, list: ActivityEntry[]): void {
  const head = ['日時', '誰が', '顧客', '応募者', '項目', '内容', '種類'];
  const lines = list.map((e) => [e.at, actorOf(e), d.customers.find((c) => c.id === e.customerId)?.name ?? '', applicantOf(d, e), columnLabel(d.sheets, e.sheetId, e.column), changeText(e), kindLabel(e)].map(csvCell).join(','));
  const blob = new Blob(['﻿' + [head.map(csvCell).join(','), ...lines].join('\r\n')], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = '操作の記録（全顧客）.csv';
  a.click();
  URL.revokeObjectURL(url);
}

/** 全顧客分の操作の記録。期間・顧客・人・種類の絞り込みは表の上の 1 行に置く。 */
export function Activity() {
  const d = useData();
  const { accounts, activityAccountId, activityCustomerId } = useAccountsState();
  // 他の画面から来たときの絞り込み（アカウントの詳細・顧客の詳細）を、開いたときに 1 回だけ取り込む。
  const [customerId, setCustomerId] = useState(activityCustomerId);
  const [who, setWho] = useState(() => {
    const a = accounts.find((x) => x.id === activityAccountId);
    return a ? (a.displayName !== '' ? a.displayName : a.email) : '';
  });
  const [period, setPeriod] = useState<Period>('all');
  const [kind, setKind] = useState<ActivityKind | ''>('');
  useEffect(() => {
    setActivityAccount('');
    setActivityCustomer('');
  }, []);

  const inCustomer = useMemo(() => d.activity.filter((e) => customerId === '' || e.customerId === customerId), [d.activity, customerId]);
  const people = useMemo(() => {
    const names = new Set<string>();
    for (const e of inCustomer) {
      names.add(actorOf(e));
      if (e.accountEmail !== '') names.add(personOf(e.accountEmail));
    }
    return [...names];
  }, [inCustomer]);
  const kinds = useMemo(() => [...new Set(inCustomer.map((e) => e.kind))], [inCustomer]);
  const filtered = useMemo(
    () => [...inCustomer].sort((a, b) => Date.parse(b.at) - Date.parse(a.at)).filter((e) => inPeriod(e.at, period) && (who === '' || involves(e, who)) && (kind === '' || e.kind === kind)),
    [inCustomer, period, who, kind],
  );
  const groups = useMemo(() => {
    const m = new Map<string, ActivityEntry[]>();
    for (const e of filtered) m.set(dayKey(e.at), [...(m.get(dayKey(e.at)) ?? []), e]);
    return [...m.entries()];
  }, [filtered]);
  const showCustomer = customerId === '';
  const cols = cx(GRID_BASE, showCustomer ? COLS_ALL : COLS_ONE);
  const customerName = (id: string): string => d.customers.find((c) => c.id === id)?.name ?? '';
  const reset = (): void => { setPeriod('all'); setWho(''); setKind(''); setCustomerId(''); };

  return (
    <AdminPage title="操作の記録" current="activity">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <Segmented<Period>
          size="sm"
          value={period}
          onChange={setPeriod}
          options={[{ value: 'today', label: '今日' }, { value: 'week', label: '7日' }, { value: 'month', label: '30日' }, { value: 'all', label: '全部' }]}
        />
        <Select aria-label="顧客" value={customerId} onChange={(e) => { setCustomerId(e.target.value); setWho(''); setKind(''); }} className="!w-52 sm:!h-8">
          <option value="">すべての顧客</option>
          {d.customers.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
        </Select>
        <Select aria-label="人" value={who} onChange={(e) => { setWho(e.target.value); }} className="!w-40 sm:!h-8">
          <option value="">全員</option>
          {people.map((p) => <option key={p} value={p}>{p}</option>)}
        </Select>
        <Select aria-label="種類" value={kind} onChange={(e) => { setKind(e.target.value as ActivityKind | ''); }} className="!w-40 sm:!h-8">
          <option value="">すべての種類</option>
          {kinds.map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
        </Select>
        <span className="pt-num ml-auto text-xs text-pt-muted">{String(filtered.length)} 件</span>
        <Button size="sm" disabled={filtered.length === 0} onClick={() => { exportCsv(getData(), filtered); log({ kind: 'export', detail: `記録 ${String(filtered.length)} 件を書き出し`, customerId: customerId !== '' ? customerId : 'c1' }); }}><IconDownload size={14} />書き出し</Button>
      </div>

      {groups.length === 0 ? (
        <p className="py-10 text-center text-[13px] text-pt-muted">
          条件に合う記録はありません
          <button type="button" onClick={reset} className="ml-2 text-pt-primary hover:underline">条件をクリア</button>
        </p>
      ) : (
        <div className="max-h-[calc(100dvh-170px)] overflow-auto rounded-lg border border-pt-line bg-pt-surface">
          <div className={cx(cols, 'sticky top-0 z-20 h-8 border-b border-pt-line bg-pt-sunken text-xs font-medium text-pt-muted')}>
            <span>日時</span><span>誰が</span>{showCustomer && <span>顧客</span>}<span>応募者</span><span>項目</span><span>変更</span><span>種類</span>
          </div>
          {groups.map(([key, list]) => (
            <section key={key}>
              <h2 className="flex items-baseline gap-2 border-b border-pt-line bg-pt-bg px-3 py-1 text-xs font-semibold text-pt-muted">
                {dayLabel(key)}<span className="pt-num font-normal text-pt-faint">{String(list.length)} 件</span>
              </h2>
              <ul>
                {list.map((e) => {
                  const change = changeText(e);
                  const sheet = d.sheets.find((s) => s.id === e.sheetId)?.tabName;
                  const col = columnLabel(d.sheets, e.sheetId, e.column);
                  return (
                    <li key={e.id} className={cx(cols, 'h-9 border-b border-pt-line text-[13px] last:border-b-0 hover:bg-pt-bg')}>
                      <span className="pt-num text-xs text-pt-muted">{formatTime(e.at)}</span>
                      <span className="truncate font-medium" title={actorOf(e)}>{actorOf(e)}</span>
                      {showCustomer && <span className="truncate text-pt-muted" title={customerName(e.customerId)}>{customerName(e.customerId)}</span>}
                      <span className="truncate" title={applicantOf(d, e)}>{applicantOf(d, e)}</span>
                      <span className="truncate text-pt-muted" title={[sheet ?? '', col].filter((x) => x !== '').join(' ／ ')}>{col}</span>
                      <span className="truncate" title={change}>{change}</span>
                      <KindMark e={e} />
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      )}
    </AdminPage>
  );
}
