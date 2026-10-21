// 操作の記録の表と絞り込み。顧客の「記録」と社内の管理画面で共用する。
import { useMemo, useState } from 'react';
import { NOW } from '../../fixtures';
import { actions, getData, useData } from '../../data';
import type { DataState } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import type { ActivityEntry, ActivityKind } from '../../types';
import { KIND_DOT, KIND_LABEL, columnLabel, isAccountEntry, kindLabel, sourceLabel } from '../../activityText';
import { IconDownload, IconFilter, IconSearch } from '../../ui/icons';
import { Button, IconButton, Segmented, Select, TextInput, cx, formatTime } from '../../ui/kit';

type Period = 'today' | 'week' | 'month' | 'all';

export { KIND_DOT, KIND_LABEL } from '../../activityText';

const GRID = 'lg:grid lg:grid-cols-[56px_minmax(150px,200px)_110px_120px_minmax(0,1fr)_120px] lg:items-center lg:gap-3';

const dayKey = (iso: string): string => new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Tokyo' }).format(new Date(iso));

function dayLabel(key: string): string {
  const today = new Date(NOW);
  if (key === dayKey(NOW)) return '今日';
  if (key === dayKey(new Date(today.getTime() - 86_400_000).toISOString())) return '昨日';
  const [, m, d] = key.split('-');
  return `${String(Number(m))}/${String(Number(d))}`;
}

const STAFF_ACTED: ActivityKind[] = ['invite', 'disable', 'device_removed', 'account_unlock', 'password_reset'];

function personOf(email: string): string {
  const a = getData().accounts.find((x) => x.email === email);
  return a && a.displayName !== '' ? a.displayName : email;
}

function actorOf(e: ActivityEntry): string {
  if (e.actorName !== undefined && e.actorName !== '') return e.actorName;
  if (STAFF_ACTED.includes(e.kind)) return '弊社';
  const a = getData().accounts.find((x) => x.email === e.accountEmail);
  if (a && a.displayName !== '') return a.displayName;
  return e.accountEmail !== '' ? e.accountEmail : '社内';
}

function applicantOf(d: DataState, e: ActivityEntry): string {
  if (e.sheetId === undefined || e.rowNumber === undefined) return '';
  const sheet = d.sheets.find((s) => s.id === e.sheetId);
  const row = d.rows[e.sheetId]?.find((r) => r.rowNumber === e.rowNumber);
  return row?.values[sheet?.titleColumn ?? ''] ?? '';
}

function inPeriod(iso: string, p: Period): boolean {
  if (p === 'all') return true;
  if (p === 'today') return dayKey(iso) === dayKey(NOW);
  const diff = new Date(NOW).getTime() - new Date(iso).getTime();
  return diff <= (p === 'week' ? 7 : 30) * 86_400_000;
}

/** 本文。応募者の記録は名前を埋め込まず、列の説明だけにする。 */
function summaryOf(e: ActivityEntry): string {
  const col = columnLabel(getData().sheets, e.sheetId, e.column);
  if (STAFF_ACTED.includes(e.kind) && e.accountEmail !== '') {
    const verb = e.kind === 'invite' ? 'を招待' : e.kind === 'disable' ? 'を停止・再開' : e.kind === 'account_unlock' ? 'のロックを解除' : e.kind === 'password_reset' ? 'に再設定のリンクを発行' : 'の端末を削除';
    return `${personOf(e.accountEmail)}さん${verb}。${e.detail}`;
  }
  if (e.rowNumber !== undefined && e.column !== undefined) {
    if (e.kind === 'save_conflict') return `「${col}」はシートで先に変更されていたため保存しなかった`;
    return `「${col}」を変更`;
  }
  return e.detail;
}

function splitList(s: string): string[] {
  return s.split(/[、,]/).map((x) => x.trim()).filter((x) => x !== '');
}

function Change({ e, entries }: { e: ActivityEntry; entries: ActivityEntry[] }) {
  const before = e.before ?? '';
  const after = e.after ?? '';
  const hasDiff = e.before !== undefined || e.after !== undefined;

  if (e.kind === 'save_conflict' && hasDiff) {
    const sheetChange = entries
      .filter((x) => x.kind === 'sheet_change' && x.sheetId === e.sheetId && x.rowNumber === e.rowNumber && x.column === e.column && Date.parse(x.at) <= Date.parse(e.at))
      .sort((a, b) => Date.parse(b.at) - Date.parse(a.at))[0];
    return (
      <div className="min-w-0 text-[13px] leading-snug [overflow-wrap:anywhere]">
        <p className="text-pt-faint">入れようとした値: {after !== '' ? after : '（空）'}</p>
        {sheetChange?.after !== undefined && (
          <p className="text-pt-ink">シートの値: {sheetChange.after}{sheetChange.actorName !== undefined && <span className="text-pt-muted">・{sheetChange.actorName}が変更</span>}</p>
        )}
      </div>
    );
  }
  if (!hasDiff) return <span className="text-[13px] text-pt-muted [overflow-wrap:anywhere]">{summaryOf(e)}</span>;

  if (e.kind === 'option_replace') {
    return (
      <div className="min-w-0 text-[13px] leading-snug [overflow-wrap:anywhere]">
        <p>
          <span className="mr-1.5 text-pt-faint line-through">{before !== '' ? before : '（空）'}</span>
          <span className="mr-1.5 text-pt-faint">→</span>
          <span className="font-medium text-pt-ink">{after !== '' ? after : '（空）'}</span>
        </p>
        <p className="text-xs text-pt-faint">項目の設定による置き換え</p>
      </div>
    );
  }
  if (e.kind === 'column_change' && (before.includes('、') || after.includes('、'))) {
    const b = splitList(before);
    const a = splitList(after);
    const added = a.filter((x) => !b.includes(x));
    const removed = b.filter((x) => !a.includes(x));
    return (
      <div className="min-w-0 text-[13px] leading-snug [overflow-wrap:anywhere]">
        {added.length > 0 && <p className="font-medium text-pt-ink">追加: {added.join('、')}</p>}
        {removed.length > 0 && <p className="text-pt-faint line-through">削除: {removed.join('、')}</p>}
        {added.length === 0 && removed.length === 0 && <p className="text-pt-muted">並び順を変更</p>}
      </div>
    );
  }
  if (before !== '' && after.startsWith(before) && after.length > before.length) {
    return <div className="min-w-0 text-[13px] leading-snug [overflow-wrap:anywhere]"><span className="mr-1 text-pt-faint">＋</span><span className="font-medium text-pt-ink">{after.slice(before.length).trim()}</span></div>;
  }
  return (
    <div className="min-w-0 text-[13px] leading-snug [overflow-wrap:anywhere]">
      {before !== '' && <span className="mr-1.5 text-pt-faint line-through">{before}</span>}
      {before !== '' && <span className="mr-1.5 text-pt-faint">→</span>}
      <span className="font-medium text-pt-ink">{after !== '' ? after : '（空）'}</span>
    </div>
  );
}

function KindMark({ e }: { e: ActivityEntry }) {
  const kind = e.kind;
  const strong = kind === 'save_conflict' || kind === 'login_failed';
  if (strong) {
    return (
      <span className={cx('inline-flex items-center rounded px-1.5 py-0.5 text-xs font-medium whitespace-nowrap', kind === 'save_conflict' ? 'bg-pt-warn-soft text-pt-warn' : 'bg-pt-danger-soft text-pt-danger')}>
        {kindLabel(e)}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-muted">
      <span className={cx('h-1.5 w-1.5 rounded-full', KIND_DOT[kind])} />{kindLabel(e)}
    </span>
  );
}

function csvCell(v: string): string {
  return `"${v.replace(/"/g, '""')}"`;
}

export function exportCsv(d: DataState, entries: ActivityEntry[]): void {
  const head = ['日時', '誰が', 'どこから', '応募者', '項目', '前', '後', '種類', '内容'];
  const lines = entries.map((e) =>
    [
      e.at,
      actorOf(e),
      sourceLabel(e),
      applicantOf(d, e),
      columnLabel(d.sheets, e.sheetId, e.column),
      e.before ?? '',
      e.after ?? '',
      kindLabel(e),
      summaryOf(e),
    ].map(csvCell).join(','),
  );
  const blob = new Blob(['﻿' + [head.map(csvCell).join(','), ...lines].join('\r\n')], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = '操作の記録.csv';
  a.click();
  URL.revokeObjectURL(url);
}

/** 「変更だけ」で残す記録。閲覧・ログイン・確認コード・招待などは「すべて」のときだけ出す。 */
const CHANGE_KINDS: ActivityKind[] = ['save', 'sheet_change', 'save_conflict', 'column_change', 'option_replace'];

export function ActivityTable({ entries, showCustomer = false, onExport, sheetId: fixedSheet, changesToggle = false }: {
  entries: ActivityEntry[];
  showCustomer?: boolean;
  /** 絞り込んだ一覧を渡す。 */
  onExport?: (filtered: ActivityEntry[]) => void;
  /** 指定すると、そのシートの記録だけにする（シートのタブと連動）。 */
  sheetId?: string | undefined;
  /** 「変更だけ／すべて」の切り替えを出し、既定を「変更だけ」にする。記録そのものは消さない。 */
  changesToggle?: boolean;
}) {
  const d = useData();
  const nav = usePreviewNav();
  const [period, setPeriod] = useState<Period>('all');
  const [who, setWho] = useState('');
  const [kind, setKind] = useState<ActivityKind | 'account' | ''>('');
  const [query, setQuery] = useState('');
  const [sheetPick, setSheetPick] = useState('');
  const [open, setOpen] = useState(false);
  const [changesOnly, setChangesOnly] = useState(changesToggle);

  const scoped = useMemo(() => {
    const bySheet = fixedSheet === undefined ? entries : entries.filter((e) => e.sheetId === undefined || e.sheetId === fixedSheet);
    return changesToggle && changesOnly ? bySheet.filter((e) => CHANGE_KINDS.includes(e.kind)) : bySheet;
  }, [entries, fixedSheet, changesToggle, changesOnly]);
  const people = useMemo(() => [...new Set(scoped.map(actorOf))], [scoped]);
  const kinds = useMemo(() => [...new Set(scoped.filter((e) => !isAccountEntry(e)).map((e) => e.kind))], [scoped]);
  const hasAccount = useMemo(() => scoped.some(isAccountEntry), [scoped]);
  const sheetIds = useMemo(() => [...new Set(entries.flatMap((e) => (e.sheetId !== undefined ? [e.sheetId] : [])))], [entries]);
  const manySheets = fixedSheet === undefined && sheetIds.length > 1;
  const customerName = (id: string): string => d.customers.find((c) => c.id === id)?.name ?? '';
  const sheetName = (id: string | undefined): string => (id === undefined ? '' : d.sheets.find((s) => s.id === id)?.tabName ?? '');

  const filtered = useMemo(() => {
    const q = query.trim();
    return [...scoped]
      .sort((a, b) => Date.parse(b.at) - Date.parse(a.at))
      .filter((e) => inPeriod(e.at, period) && (who === '' || actorOf(e) === who) && (kind === '' || (kind === 'account' ? isAccountEntry(e) : e.kind === kind && !isAccountEntry(e))) && (fixedSheet !== undefined || sheetPick === '' || e.sheetId === sheetPick) && (q === '' || applicantOf(d, e).includes(q)));
  }, [scoped, period, who, kind, fixedSheet, sheetPick, query, d]);

  const groups = useMemo(() => {
    const m = new Map<string, ActivityEntry[]>();
    for (const e of filtered) {
      const k = dayKey(e.at);
      m.set(k, [...(m.get(k) ?? []), e]);
    }
    return [...m.entries()];
  }, [filtered]);

  const extraCount = (kind !== '' ? 1 : 0) + (query.trim() !== '' ? 1 : 0) + (fixedSheet === undefined && sheetPick !== '' ? 1 : 0);
  const anyFilter = extraCount > 0 || who !== '' || period !== 'all';
  const reset = (): void => { setKind(''); setQuery(''); setSheetPick(''); setWho(''); setPeriod('all'); };

  const openApplicant = (e: ActivityEntry): void => {
    if (e.sheetId !== undefined) actions.selectSheet(e.sheetId);
    nav.go(ROUTES.list);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
        <div className="flex items-center gap-2">
          {changesToggle && (
            <Segmented<'changes' | 'all'>
              size="sm"
              value={changesOnly ? 'changes' : 'all'}
              onChange={(v) => { setChangesOnly(v === 'changes'); setKind(''); }}
              options={[{ value: 'changes', label: '変更だけ' }, { value: 'all', label: 'すべて' }]}
            />
          )}
          <Segmented<Period>
            size="sm"
            value={period}
            onChange={setPeriod}
            options={[{ value: 'today', label: '今日' }, { value: 'week', label: '7日' }, { value: 'month', label: '30日' }, { value: 'all', label: '全部' }]}
          />
          <span className="pt-num ml-auto text-xs text-pt-muted sm:hidden">{String(filtered.length)} 件</span>
        </div>
        <div className="flex flex-1 items-center gap-2">
          <Select aria-label="人" value={who} onChange={(e) => { setWho(e.target.value); }} className="!h-11 min-w-0 flex-1 !rounded-md sm:!h-8 sm:max-w-44 sm:flex-none">
            <option value="">全員</option>
            {people.map((p) => <option key={p} value={p}>{p}</option>)}
          </Select>
          <Button size="sm" className="!h-11 sm:!h-8" onClick={() => { setOpen(!open); }} aria-expanded={open}>
            <IconFilter size={14} />絞り込み{extraCount > 0 && <span className="pt-num rounded-full bg-pt-primary px-1.5 text-[11px] text-white">{String(extraCount)}</span>}
          </Button>
          <span className="pt-num ml-auto hidden text-xs text-pt-muted sm:inline">{String(filtered.length)} 件</span>
          {onExport && (
            <>
              <IconButton label="書き出し" className="!h-11 !w-11 sm:hidden" onClick={() => { onExport(filtered); }} disabled={filtered.length === 0}><IconDownload size={16} /></IconButton>
              <Button size="sm" className="!hidden sm:!inline-flex" onClick={() => { onExport(filtered); }} disabled={filtered.length === 0}><IconDownload size={14} />書き出し</Button>
            </>
          )}
        </div>
      </div>

      {open && (
        <div className="grid gap-3 rounded-md border border-pt-line bg-pt-surface p-3 sm:grid-cols-4">
          <label className="flex flex-col gap-1 text-xs text-pt-muted">種類
            <Select value={kind} onChange={(e) => { setKind(e.target.value as ActivityKind | 'account' | ''); }}>
              <option value="">すべて</option>
              {kinds.map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
              {hasAccount && <option value="account">アカウント（ログイン・パスワード・端末）</option>}
            </Select>
          </label>
          {fixedSheet === undefined && (
            <label className="flex flex-col gap-1 text-xs text-pt-muted">シート
              <Select value={sheetPick} onChange={(e) => { setSheetPick(e.target.value); }}>
                <option value="">すべて</option>
                {sheetIds.map((id) => <option key={id} value={id}>{sheetName(id) || id}</option>)}
              </Select>
            </label>
          )}
          <label className={cx('flex flex-col gap-1 text-xs text-pt-muted', fixedSheet === undefined ? 'sm:col-span-2' : 'sm:col-span-3')}>応募者の名前
            <span className="relative">
              <IconSearch size={14} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-pt-faint" />
              <TextInput value={query} onChange={(e) => { setQuery(e.target.value); }} placeholder="名前の一部" className="!pl-8" />
            </span>
          </label>
          <div className="sm:col-span-4"><Button size="sm" variant="ghost" onClick={reset}>条件をクリア</Button></div>
        </div>
      )}

      {groups.length === 0 ? (
        <p className="py-10 text-center text-[13px] text-pt-muted">
          条件に合う記録はありません
          {anyFilter && <button type="button" onClick={reset} className="ml-2 text-pt-primary hover:underline">条件をクリア</button>}
        </p>
      ) : (
        <div className="max-h-[calc(100dvh-210px)] overflow-auto rounded-md border border-pt-line bg-pt-surface">
          <div className={cx('sticky top-0 z-20 hidden border-b border-pt-line bg-pt-surface px-3 py-1.5 text-xs text-pt-faint', GRID)}>
            <span>日時</span><span>誰が</span><span>応募者</span><span>項目</span><span>変更</span><span>種類</span>
          </div>
          {groups.map(([key, list]) => (
            <section key={key}>
              <h2 className="sticky top-0 z-10 flex items-baseline gap-2 bg-pt-sunken px-3 py-1 text-xs font-semibold text-pt-muted lg:top-[29px]">
                {dayLabel(key)}<span className="pt-num font-normal text-pt-faint">{String(list.length)} 件</span>
              </h2>
              <ul className="divide-y divide-pt-line">
                {list.map((e) => {
                  const app = applicantOf(d, e);
                  const src = sourceLabel(e);
                  const col = columnLabel(d.sheets, e.sheetId, e.column);
                  const hasApp = app !== '' || e.column !== undefined;
                  const sub = [manySheets ? sheetName(e.sheetId) : '', showCustomer ? customerName(e.customerId) : ''].filter((x) => x !== '').join(' ・ ');
                  const appBtn = app !== '' && (
                    <button type="button" onClick={() => { openApplicant(e); }} className="max-w-full truncate text-left text-pt-primary hover:underline">{app}</button>
                  );
                  return (
                    <li key={e.id} className={cx('px-3 py-2 text-[13px]', GRID)}>
                      <div className="flex items-center gap-2 lg:contents">
                        <span className="pt-num w-11 shrink-0 text-xs text-pt-muted">{formatTime(e.at)}</span>
                        <span className="flex min-w-0 items-baseline gap-1">
                          <span className="truncate font-medium">{actorOf(e)}</span>
                          <span className="shrink-0 text-xs text-pt-faint">・{src}</span>
                        </span>
                        <span className="ml-auto lg:hidden"><KindMark e={e} /></span>
                        {hasApp ? (
                          <>
                            <span className="hidden min-w-0 lg:block">{appBtn}</span>
                            <span className="hidden truncate text-pt-muted lg:block">{col}</span>
                          </>
                        ) : null}
                        <div className={cx('hidden min-w-0 lg:block', !hasApp && 'lg:col-span-3')}>
                          <Change e={e} entries={entries} />
                          {sub !== '' && <p className="text-xs text-pt-faint">{sub}</p>}
                        </div>
                        <span className="hidden lg:block"><KindMark e={e} /></span>
                      </div>
                      <div className="mt-1 flex flex-col gap-0.5 lg:hidden">
                        {hasApp && (
                          <p className="flex min-w-0 items-baseline gap-1">
                            {appBtn}
                            {app !== '' && e.column !== undefined && <span className="text-pt-faint">・</span>}
                            <span className="truncate text-pt-muted">{col}</span>
                          </p>
                        )}
                        <Change e={e} entries={entries} />
                        {sub !== '' && <p className="text-xs text-pt-faint">{sub}</p>}
                      </div>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
