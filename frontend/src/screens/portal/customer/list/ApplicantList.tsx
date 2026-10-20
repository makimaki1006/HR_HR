// 応募者の一覧。顧客のメイン画面。PC は表、スマホはカード。
import { useEffect, useMemo, useRef, useState } from 'react';
import { usePortalStore } from '../../store';
import { ROUTES, usePreviewNav } from '../../preview';
import { CustomerLayout } from '../../ui/layouts';
import { Button, Card, Notice, TextInput, cx, formatTime } from '../../ui/kit';
import { ContactBox } from '../../ui/contact';
import { cardLayout, labelOf, searchColumns, sortByDate, type CardLayout } from '../../columns';
import { CardBody, ChangeBadge, StatusBadge } from './ApplicantCard';
import { RowEditor } from '../edit/RowEditor';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';

export type ListState = 'normal' | 'empty' | 'unavailable' | 'stale';

const norm = (s: string) => s.replace(/[-\s]/g, '').toLowerCase();
const PAGE_SIZE = 30;

function Cell({ col, row, layout }: { col: ColumnConfig; row: ApplicantRow; layout: CardLayout }) {
  const v = row.values[col.header] ?? '';
  if (col === layout.status) return <StatusBadge value={v} />;
  if (col === layout.title) {
    return (
      <span className="flex items-center gap-2">
        <span className="font-medium text-pt-ink">{v}</span>
        <ChangeBadge kind={row.changedSinceLastView} />
      </span>
    );
  }
  return <>{v}</>;
}

function StateBox({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card className="px-6 py-12 text-center">
      <p className="text-base font-semibold text-pt-ink">{title}</p>
      <div className="mx-auto mt-2 max-w-md text-sm leading-relaxed text-pt-muted">{children}</div>
    </Card>
  );
}

type Retry = 'idle' | 'loading' | 'failed';

export function ApplicantList({ forceState }: { forceState?: ListState }) {
  const store = usePortalStore();
  const nav = usePreviewNav();
  const sheets = store.mySheets;
  const [sheetId, setSheetId] = useState(sheets[0]?.id ?? '');
  const sheet: SheetConfig | undefined = sheets.find((s) => s.id === sheetId) ?? sheets[0];
  const snap = store.snapshot(sheet?.id ?? '');
  const [status, setStatus] = useState('すべて');
  const [query, setQuery] = useState('');
  const [openRow, setOpenRow] = useState<number | null>(null);
  const [toast, setToast] = useState(false);
  const [flash, setFlash] = useState<Set<number>>(new Set());
  const [retry, setRetry] = useState<Retry>('idle');
  const [retryWillSucceed, setRetryWillSucceed] = useState(true);
  const [recovered, setRecovered] = useState(false);
  const prev = useRef<Map<string, { readAt: string; sig: Map<number, string> }>>(new Map());
  const tableRef = useRef<HTMLDivElement>(null);
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const on = () => { setScrolled(window.scrollY > 700); };
    window.addEventListener('scroll', on, { passive: true });
    return () => { window.removeEventListener('scroll', on); };
  }, []);

  // 読んだ時刻が変わっても、行が増えた・変わったときだけ色と通知を出す。タブごとに前回の内容を覚える。
  useEffect(() => {
    const id = sheet?.id ?? '';
    const sig = new Map(snap.rows.map((r) => [r.rowNumber, JSON.stringify(r.values)]));
    const before = prev.current.get(id);
    prev.current.set(id, { readAt: snap.readAt, sig });
    if (!before || before.readAt === snap.readAt) return undefined;
    const changed = new Set<number>();
    sig.forEach((v, k) => { if (before.sig.get(k) !== v) changed.add(k); });
    if (changed.size === 0) return undefined;
    setFlash(changed);
    setToast(true);
    const t = window.setTimeout(() => { setToast(false); setFlash(new Set()); }, 12000);
    return () => { window.clearTimeout(t); };
  }, [sheet?.id, snap.readAt, snap.rows]);

  const hasRead = snap.rows.length > 0;
  const state: ListState = (() => {
    if (forceState === 'empty') return 'empty';
    if (forceState === 'unavailable') return 'unavailable';
    if (forceState === 'stale') return recovered ? 'normal' : 'stale';
    if (sheet?.connection !== 'ok') return hasRead ? 'stale' : 'unavailable';
    return 'normal';
  })();
  const layout = useMemo(() => cardLayout(sheet ?? { columns: [] }), [sheet]);
  const allRows = useMemo(() => (state === 'empty' || state === 'unavailable' ? [] : sortByDate(snap.rows, layout.sort?.header)), [snap.rows, state, layout.sort?.header]);

  const statusKey = layout.status?.header;
  const statuses = useMemo(() => {
    if (!layout.status || !statusKey) return [];
    const base = layout.status.options?.filter((o) => o !== '') ?? [];
    const seen = allRows.map((r) => r.values[statusKey] ?? '').filter((v) => v !== '' && !base.includes(v));
    return [...base, ...new Set(seen)];
  }, [layout.status, statusKey, allRows]);
  const count = (s: string) => (s === 'すべて' ? allRows.length : allRows.filter((r) => r.values[statusKey ?? ''] === s).length);

  const searchCols = useMemo(() => searchColumns(sheet ?? { columns: [] }), [sheet]);
  const rows = useMemo(() => {
    const q = norm(query);
    return allRows.filter((r) => {
      if (statusKey && status !== 'すべて' && r.values[statusKey] !== status) return false;
      if (q === '') return true;
      return searchCols.some((c) => norm(r.values[c.header] ?? '').includes(q));
    });
  }, [allRows, status, query, statusKey, searchCols]);
  const shownRows = rows.slice(0, limit);

  const cols = (sheet?.columns ?? []).filter((c) => c.visible && c.inList);
  const selected = openRow === null ? undefined : snap.rows.find((r) => r.rowNumber === openRow);

  const open = (r: ApplicantRow) => {
    if (sheet) store.markSeen(sheet.id, r.rowNumber);
    setOpenRow(r.rowNumber);
  };

  const go = (to: 'list' | 'account' | 'logout') => {
    nav.go(to === 'list' ? ROUTES.list : to === 'account' ? ROUTES.account : ROUTES.loggedOut);
  };

  const doRetry = () => {
    setRetry('loading');
    window.setTimeout(() => {
      if (retryWillSucceed) { setRecovered(true); setRetry('idle'); } else { setRetry('failed'); }
    }, 1200);
  };

  const toTop = () => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
    tableRef.current?.scrollTo({ top: 0, behavior: 'smooth' });
    setStatus('すべて');
    setQuery('');
    setLimit(PAGE_SIZE);
    setToast(false);
  };

  const toSearch = () => {
    window.scrollTo({ top: 0, behavior: 'smooth' });
    window.setTimeout(() => { document.getElementById('applicant-search')?.focus(); }, 350);
  };

  return (
    <CustomerLayout customerName={store.customerName} accountLabel={store.account.displayName || store.account.email} current="list" onNavigate={go}>
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-end gap-x-4 gap-y-2">
          <h1 className="text-xl font-semibold text-pt-ink">応募者の一覧</h1>
          {state !== 'unavailable' && (
            <p className="pb-0.5 text-xs text-pt-muted">{formatTime(snap.readAt)} 時点</p>
          )}
        </div>

        {recovered && forceState === 'stale' && state === 'normal' && (
          <Notice tone="ok">最新の内容を読み込みました。</Notice>
        )}

        {sheets.length > 1 && (
          <div role="tablist" className="-mb-1 flex gap-1 overflow-x-auto border-b border-pt-line">
            {sheets.map((s) => (
              <button
                key={s.id}
                type="button"
                role="tab"
                aria-selected={s.id === sheet?.id}
                onClick={() => { setSheetId(s.id); setStatus('すべて'); setQuery(''); setLimit(PAGE_SIZE); setToast(false); setFlash(new Set()); }}
                className={cx('h-11 whitespace-nowrap border-b-2 px-4 text-sm', s.id === sheet?.id ? 'border-pt-primary font-medium text-pt-primary' : 'border-transparent text-pt-muted hover:text-pt-ink')}
              >
                {s.tabName}
              </button>
            ))}
          </div>
        )}

        {state === 'stale' && (
          <div className="flex flex-col gap-1">
            <Notice
              tone="warn"
              title={
                retry === 'failed'
                  ? `まだ読み込めません。${formatTime(snap.readAt)} 時点の内容のままです。`
                  : `最新の内容を読み込めませんでした。${formatTime(snap.readAt)} 時点の内容を表示しています。`
              }
              action={
                <Button size="sm" disabled={retry === 'loading'} onClick={doRetry}>
                  {retry === 'loading' ? '読み込んでいます…' : 'もう一度読み込む'}
                </Button>
              }
            >
              このあと自動でもう一度試します。この間に応募者の追加や変更があっても、ここには出ません。
            </Notice>
            {forceState === 'stale' && (
              <p className="flex items-center gap-2 text-xs text-pt-faint">
                試作の操作: 次の読み込みは
                <button type="button" onClick={() => { setRetryWillSucceed(true); }} className={cx('underline', retryWillSucceed && 'font-semibold text-pt-ink')}>成功する</button>
                <button type="button" onClick={() => { setRetryWillSucceed(false); }} className={cx('underline', !retryWillSucceed && 'font-semibold text-pt-ink')}>失敗する</button>
              </p>
            )}
          </div>
        )}

        {state === 'unavailable' && (
          <StateBox title="いま応募者の一覧を読み込めません">
            <p>弊社で確認しています。復旧しだい、この画面に表示されます。</p>
            <p className="mt-2">お急ぎのときは、担当者までご連絡ください。</p>
            <div className="mx-auto mt-5 max-w-sm text-left"><ContactBox mailSubject="応募者の一覧が読み込めません" /></div>
          </StateBox>
        )}

        {state === 'empty' && (
          <StateBox title="応募者はまだいません">
            <p>応募があると、ここに表示されます。</p>
          </StateBox>
        )}

        {(state === 'normal' || state === 'stale') && (
          <>
            <div className="sm:max-w-sm">
              <TextInput id="applicant-search" type="search" value={query} placeholder="名前・電話番号で探す" aria-label="検索" onChange={(e) => { setQuery(e.target.value); setLimit(PAGE_SIZE); }} />
            </div>
            {statuses.length > 0 && <div className="sticky top-14 z-10 -mx-4 bg-pt-bg py-2 sm:static sm:mx-0 sm:py-0">
              <div className="flex gap-2 overflow-x-auto px-4 sm:px-0" role="group" aria-label="選考状況で絞り込み">
                {['すべて', ...statuses].map((s) => (
                  <button
                    key={s}
                    type="button"
                    aria-pressed={status === s}
                    onClick={() => { setStatus(s); setLimit(PAGE_SIZE); }}
                    className={cx('inline-flex h-11 shrink-0 items-center gap-1.5 rounded-full border px-4 text-sm', status === s ? 'border-pt-primary bg-pt-primary text-white' : 'border-pt-line-strong bg-pt-surface text-pt-ink hover:bg-pt-bg')}
                  >
                    {s}
                    <span className={cx('text-xs tabular-nums', status === s ? 'text-white/80' : 'text-pt-muted')}>{count(s)}</span>
                  </button>
                ))}
              </div>
            </div>}

            {rows.length === 0 ? (
              <StateBox title="該当する応募者がいません">
                <p>絞り込みや検索の言葉を変えてください。</p>
                <div className="mt-3"><Button onClick={() => { setStatus('すべて'); setQuery(''); setLimit(PAGE_SIZE); }}>条件を解除</Button></div>
              </StateBox>
            ) : (
              <>
                <p className="text-xs text-pt-muted">{String(rows.length)} 件{layout.sort ? `（${labelOf(layout.sort)}の新しい順）` : ''}</p>

                {/* PC: 表 */}
                <Card className="hidden overflow-hidden sm:block">
                  <div ref={tableRef} className="max-h-[calc(100dvh-22rem)] min-h-64 overflow-auto">
                    <table className="w-full border-collapse text-sm">
                      <thead>
                        <tr>
                          {cols.map((c) => (
                            <th key={c.header} className="sticky top-0 z-10 whitespace-nowrap border-b border-pt-line bg-pt-bg px-3 py-2.5 text-left text-xs font-medium text-pt-muted">{labelOf(c)}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {rows.map((r) => (
                          <tr
                            key={r.rowNumber}
                            tabIndex={0}
                            aria-selected={openRow === r.rowNumber}
                            onClick={() => { open(r); }}
                            onKeyDown={(e) => { if (e.key === 'Enter') open(r); }}
                            className={cx(
                              'h-12 cursor-pointer border-b border-pt-line last:border-b-0 hover:bg-pt-bg',
                              openRow === r.rowNumber ? 'bg-pt-primary-soft' : flash.has(r.rowNumber) && 'bg-pt-warn-soft',
                            )}
                          >
                            {cols.map((c) => (
                              <td key={c.header} className="max-w-[16rem] truncate whitespace-nowrap px-3 text-pt-ink"><Cell col={c} row={r} layout={layout} /></td>
                            ))}
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </Card>

                {/* スマホ: カード */}
                <ul className="flex flex-col gap-2 sm:hidden">
                  {shownRows.map((r) => (
                    <li key={r.rowNumber}>
                      <button
                        type="button"
                        onClick={() => { open(r); }}
                        className={cx(
                          'w-full rounded-lg border p-3 text-left',
                          openRow === r.rowNumber ? 'border-pt-primary bg-pt-primary-soft' : flash.has(r.rowNumber) ? 'border-pt-line bg-pt-warn-soft' : 'border-pt-line bg-pt-surface',
                        )}
                      >
                        <CardBody layout={layout} values={r.values} changed={r.changedSinceLastView} />
                      </button>
                    </li>
                  ))}
                </ul>
                {rows.length > shownRows.length && (
                  <div className="flex flex-col items-center gap-1 sm:hidden">
                    <Button className="h-11 w-full" onClick={() => { setLimit((n) => n + PAGE_SIZE); }}>もっと見る（残り {String(rows.length - shownRows.length)} 件）</Button>
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>

      {toast && (
        <div role="status" className="fixed inset-x-4 bottom-4 z-30 mx-auto flex max-w-md items-center gap-3 rounded-lg border border-pt-primary bg-pt-surface px-4 py-3 text-sm text-pt-ink shadow-lg">
          <span className="min-w-0 flex-1">シートの内容が更新されました。変わった行に色を付けています。</span>
          <Button size="sm" variant="primary" onClick={toTop}>新着を見る</Button>
          <button type="button" aria-label="閉じる" onClick={() => { setToast(false); }} className="h-8 shrink-0 px-1 text-pt-muted hover:text-pt-ink">閉じる</button>
        </div>
      )}

      {scrolled && !selected && (
        <button type="button" onClick={toSearch} className="fixed bottom-4 right-4 z-20 h-11 rounded-full border border-pt-line-strong bg-pt-surface px-4 text-sm font-medium text-pt-ink shadow-md sm:hidden">上へ・検索</button>
      )}

      {sheet && selected && (
        <RowEditor sheet={sheet} row={selected} onClose={() => { setOpenRow(null); }} />
      )}
    </CustomerLayout>
  );
}
