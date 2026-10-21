// 応募者の一覧。顧客のメイン画面。PC はスプレッドシートのように書き換えられる表、スマホはカード。
import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { actions, currentSheet, getListPrefs, readListUrl, useData, writeListUrl } from '../../data';
import { cardLayout, kindOf, labelOf, searchColumns } from '../../columns';
import { AppShell, type SyncInfo } from '../../ui/shell';
import { ContactBox } from '../../ui/contact';
import { IconColumns, IconDownload, IconSearch, IconUsers, IconX } from '../../ui/icons';
import { BottomSheet, Button, cx } from '../../ui/kit';
import { RowEditor } from '../edit/RowEditor';
import { setVisibleRows } from '../edit/visibleRows';
import { ROUTES, usePreviewNav } from '../../preview';
import type { ListPrefs, ListUrlState, SheetConfig } from '../../types';
import { Grid } from './Grid';
import { MobileList } from './MobileList';
import { downloadText, matchesFilters, matchesQuery, omit, orderColumns, sortRows, statusDot, toCsv, type ColumnFilter, type SortSpec } from './model';
import { ToastBar, useOutside } from './parts';
import { matchesQuick, noInterviewNote, quickChips, SortSelect, TabScroller, TodayBand, type Quick } from './quick';
import { TodayInterviews } from './TodayInterviews';
import { useCellSaver } from './useCellSaver';
import { NOW } from '../../fixtures';

export type ListState = 'normal' | 'empty' | 'unavailable' | 'stale';

const SHEET_NOTE = 'シートの内容が更新されました';
const DESKTOP = '(min-width: 1024px)';
function useIsDesktop(): boolean {
  return useSyncExternalStore(
    (cb) => { const m = window.matchMedia(DESKTOP); m.addEventListener('change', cb); return () => { m.removeEventListener('change', cb); }; },
    () => window.matchMedia(DESKTOP).matches,
  );
}

export function ApplicantList({ forceState, openAtStart }: { forceState?: ListState; /** 最初から詳細を開いておく行（試作の画面用）。 */ openAtStart?: number | undefined }) {
  const d = useData();
  const sheet = currentSheet(d);
  const [recovered, setRecovered] = useState(false);
  const state: ListState = forceState === 'stale' ? (recovered ? 'normal' : 'stale') : forceState ?? (sheet?.connection !== 'ok' ? 'unavailable' : 'normal');
  const readAt = sheet ? (d.readAt[sheet.id] ?? NOW) : NOW;
  const sync: SyncInfo | undefined = state === 'unavailable'
    ? { state: 'unavailable' }
    : state === 'stale'
      ? { state: 'stale', at: new Date(new Date(readAt).getTime() - 25 * 60_000).toISOString(), onReload: () => { setRecovered(true); } }
      : undefined;
  return (
    <AppShell section="applicants" title="応募者" flush sync={sync} mobileHeader="compact">
      {sheet ? <ListBody key={sheet.id} sheet={sheet} state={state} openAtStart={openAtStart} /> : <Unavailable sheet={undefined} />}
    </AppShell>
  );
}

/** 社内の人に出す、読めない理由。顧客には出さない。 */
const REASON: Record<SheetConfig['connection'], string> = {
  ok: '',
  not_shared: 'このシートは共有されていません。シートの共有に、読み取り用のアカウントが入っているか確かめてください。',
  header_changed: 'シートの見出しの行が変わっています。見出しの行の番号と、列の名前が設定と合っているか確かめてください。',
  not_found: 'シートが見つかりません。削除されたか、タブの名前が変わった可能性があります。',
};

function Unavailable({ sheet }: { sheet: SheetConfig | undefined }) {
  const d = useData();
  const nav = usePreviewNav();
  if (d.viewer.kind === 'staff') {
    const customerId = sheet?.customerId ?? d.currentCustomerId;
    return (
      <div className="mx-auto max-w-md px-4 py-14">
        <span className="mb-4 inline-flex h-9 w-9 items-center justify-center rounded-full bg-pt-warn-soft text-pt-warn"><IconUsers size={18} /></span>
        <h2 className="text-[15px] font-semibold">このシートを読み込めていません</h2>
        <p className="mt-1 text-[13px] text-pt-muted">{sheet ? REASON[sheet.connection] : 'この顧客にはシートがまだ登録されていません。'}</p>
        <p className="mt-2 text-[12px] text-pt-faint">顧客の担当者には、サポート窓口の案内が出ています。</p>
        <div className="mt-5">
          <Button
            variant="primary"
            onClick={() => { actions.selectCustomer(customerId); nav.go('admin-customer-detail'); }}
          >
            顧客の詳細で確かめる
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="mx-auto max-w-md px-4 py-14">
      <span className="mb-4 inline-flex h-9 w-9 items-center justify-center rounded-full bg-pt-danger-soft text-pt-danger"><IconUsers size={18} /></span>
      <h2 className="text-[15px] font-semibold">応募者の一覧を表示できません</h2>
      <p className="mt-1 text-[13px] text-pt-muted">シートを読み込めませんでした。お手数ですが、下の窓口までご連絡ください。</p>
      <div className="mt-5"><ContactBox mailSubject="応募者の一覧が表示できません" /></div>
    </div>
  );
}

function Empty() {
  return (
    <div className="mx-auto max-w-md px-4 py-16 text-center">
      <span className="mx-auto mb-4 flex h-10 w-10 items-center justify-center rounded-full bg-pt-sunken text-pt-muted"><IconUsers size={20} /></span>
      <h2 className="text-[15px] font-semibold">応募者はまだいません</h2>
      <p className="mt-1 text-[13px] text-pt-muted">応募があると、ここに並びます。</p>
    </div>
  );
}

function ListBody({ sheet, state, openAtStart }: { sheet: SheetConfig; state: ListState; openAtStart?: number | undefined }) {
  const d = useData();
  const isDesktop = useIsDesktop();
  const saver = useCellSaver(sheet.id);
  // URL に入れた状態（状況タブ・チップ・検索・並び・開いている応募者）を先に読み、無ければ覚えている設定を使う。
  const [urlInit] = useState(() => readListUrl());
  const [query, setQuery] = useState(urlInit.q ?? '');
  const [status, setStatus] = useState<string | null>(urlInit.status ?? null);
  // 列の表示・並べ替え・絞り込みは、人とシートごとに覚えて、次に開いたときも同じにする。
  const [saved] = useState(() => getListPrefs(d, sheet.id));
  const [filters, setFilters] = useState<Record<string, ColumnFilter>>(saved?.filters ?? {});
  const [sort, setSort] = useState<SortSpec | null>(() => {
    if (urlInit.sort === undefined) return saved?.sort ?? null;
    if (urlInit.sort === 'near' && sheet.interviewColumn) return { column: sheet.interviewColumn, dir: 'asc', mode: 'near' };
    if (urlInit.sort === 'changed') return { column: '', dir: 'desc', mode: 'changed' };
    return null;
  });
  const [shownOverride, setShownOverride] = useState<Record<string, boolean>>(saved?.shown ?? {});
  const [quick, setQuick] = useState<Quick | null>(urlInit.quick ?? null);
  // チップ（今日・明日の面接）で自動に変えた並びは覚えない。本人が並びを選んだら、そちらを使う。
  const [sortTouched, setSortTouched] = useState(false);
  const pickQuick = (q: Quick | null) => { setQuick(q); setSortTouched(false); };
  const [pendingNote, setPendingNote] = useState(false);
  const remember = (next: Partial<ListPrefs>) => {
    actions.setListPrefs(sheet.id, { shown: next.shown ?? shownOverride, sort: next.sort === undefined ? sort : next.sort, filters: next.filters ?? filters });
  };
  const changeFilters = (next: Record<string, ColumnFilter>) => { setFilters(next); remember({ filters: next }); };
  const changeSort = (next: SortSpec | null) => { setSort(next); setSortTouched(true); remember({ sort: next }); };
  const changeShown = (next: Record<string, boolean>) => { setShownOverride(next); remember({ shown: next }); };
  const [openRow, setOpenRow] = useState<number | null>(urlInit.row ?? openAtStart ?? null);
  const [jump, setJump] = useState<{ rowNumber: number; seq: number } | null>(null);
  const nav = usePreviewNav();
  const pushedRef = useRef(false);
  const scrollRef = useRef(0);
  const canEditColumns = d.viewer.kind === 'staff' || sheet.customerCanEditColumns === true;

  const sourceRows = useMemo(() => (state === 'empty' || state === 'unavailable' ? [] : d.rows[sheet.id] ?? []), [d.rows, sheet.id, state]);
  const layout = cardLayout(sheet);
  const titleHeader = layout.title?.header ?? '';
  const statusCol = layout.status;
  // 検索の対象: 名前・電話などに、日付の列（「10/23」でも当たる）を足す。
  const searchCols = useMemo(() => {
    const base = searchColumns(sheet);
    return [...base, ...sheet.columns.filter((c) => c.visible && (kindOf(c) === 'date' || kindOf(c) === 'datetime') && !base.includes(c))];
  }, [sheet]);

  // シートで社内の人が書き換えたら、表は差し替わり、下に知らせを出す。
  const readAt = d.readAt[sheet.id] ?? '';
  const [seenReadAt, setSeenReadAt] = useState(readAt);
  const [seenRows, setSeenRows] = useState(() => new Set(sourceRows.map((r) => r.rowNumber)));
  if (readAt !== seenReadAt) {
    setSeenReadAt(readAt);
    setSeenRows(new Set(sourceRows.map((r) => r.rowNumber)));
    const added = sourceRows.filter((r) => !seenRows.has(r.rowNumber));
    const fromSheet = d.activity.some((e) => e.sheetId === sheet.id && e.source === 'sheet' && e.at > seenReadAt
      && !(e.rowNumber !== undefined && e.column !== undefined && saver.cells[`${String(e.rowNumber)}|${e.column}`]?.kind === 'saving'));
    const first = added[0];
    if (first) {
      saver.notify(`新しい応募者が ${String(added.length)} 人入りました`, {
        label: '見る',
        run: () => {
          setQuery(''); setStatus(null); setFilters({}); setSort(null); pickQuick(null);
          setJump((j) => ({ rowNumber: first.rowNumber, seq: (j?.seq ?? 0) + 1 }));
        },
      });
    } else if (fromSheet) {
      // 競合・失敗の吹き出しが出ているあいだは、知らせを重ねない。吹き出しが閉じてから 1 回だけ出す。
      if (saver.bubble !== null) setPendingNote(true);
      else saver.notify(SHEET_NOTE);
    }
  }
  /* eslint-disable react-hooks/set-state-in-effect, react-hooks/exhaustive-deps -- 吹き出しの開閉に合わせて、知らせを 1 つにする */
  useEffect(() => {
    if (saver.bubble !== null) {
      if (saver.toast?.text === SHEET_NOTE) { saver.closeToast(); setPendingNote(true); }
    } else if (pendingNote) {
      setPendingNote(false);
      saver.notify(SHEET_NOTE);
    }
  }, [saver.bubble, saver.toast, pendingNote]);
  /* eslint-enable react-hooks/set-state-in-effect, react-hooks/exhaustive-deps */

  // Ctrl+Z で直前の保存を元に戻す（入力中は入力欄の取り消しに任せる）。
  const undoRef = useRef(saver.undo);
  useEffect(() => { undoRef.current = saver.undo; }, [saver.undo]);
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== 'z' || e.shiftKey) return;
      const t = e.target;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement) return;
      e.preventDefault();
      undoRef.current();
    };
    window.addEventListener('keydown', h);
    return () => { window.removeEventListener('keydown', h); };
  }, []);

  const shownCols = useMemo(() => {
    const vis = orderColumns(sheet.columns.filter((c) => c.visible), titleHeader, statusCol?.header);
    return vis.filter((c) => c.header === titleHeader || (shownOverride[c.header] ?? c.inList));
  }, [sheet.columns, titleHeader, statusCol?.header, shownOverride]);

  const baseRows = useMemo(() => sourceRows.filter((r) => matchesQuery(r, searchCols, query, sheet.sortColumn) && matchesFilters(r, filters)), [sourceRows, searchCols, query, filters, sheet.sortColumn]);
  // 状況タブの数は、チップ・検索・見出しの絞り込みを当てたあとの行で数える。
  const quickRows = useMemo(() => (quick === null ? baseRows : baseRows.filter((r) => matchesQuick(sheet, r, quick))), [baseRows, quick, sheet]);
  const statusCounts = useMemo(() => {
    const m = new Map<string, number>();
    if (statusCol) for (const r of quickRows) { const v = r.values[statusCol.header] ?? ''; m.set(v, (m.get(v) ?? 0) + 1); }
    return m;
  }, [quickRows, statusCol]);
  const chips = useMemo(() => quickChips(sheet, sourceRows), [sheet, sourceRows]);
  // 今日・明日の面接のチップを選んだら、面接の早い順に並べる（本人が並びを選び直すまで）。
  const viewSort = useMemo<SortSpec | null>(
    () => ((quick === 'today' || quick === 'tomorrow') && !sortTouched && sheet.interviewColumn ? { column: sheet.interviewColumn, dir: 'asc', mode: 'near' } : sort),
    [quick, sortTouched, sheet.interviewColumn, sort],
  );
  const rows = useMemo(() => {
    const f = status === null || !statusCol ? quickRows : quickRows.filter((r) => (r.values[statusCol.header] ?? '') === status);
    return sortRows(f, viewSort, layout.sort?.header, sheet.interviewTimeColumn);
  }, [quickRows, status, statusCol, viewSort, sheet.interviewTimeColumn, layout.sort?.header]);

  // 詳細の「前へ・次へ」と位置の表示を、一覧の並び（絞り込み・並べ替えの後）に合わせる。
  useEffect(() => { setVisibleRows(sheet.id, rows.map((r) => r.rowNumber)); }, [sheet.id, rows]);

  const [debQuery, setDebQuery] = useState(query);
  useEffect(() => {
    const t = setTimeout(() => { setDebQuery(query); }, 400);
    return () => { clearTimeout(t); };
  }, [query]);
  const sortKey = sort?.mode ?? '';
  // この画面を離れたら、一覧の状態を URL から外す。
  useEffect(() => () => { writeListUrl({}, 'replace'); }, []);
  // 戻る・進むの操作で、URL の row に合わせて詳細を開閉する。閉じたら一覧のスクロール位置に戻す。
  useEffect(() => {
    const h = () => {
      const u = readListUrl();
      pushedRef.current = u.row !== undefined;
      setOpenRow(u.row ?? null);
      if (u.row === undefined) requestAnimationFrame(() => { window.scrollTo(0, scrollRef.current); });
    };
    window.addEventListener('popstate', h);
    return () => { window.removeEventListener('popstate', h); };
  }, []);
  // URL の row が一覧に無い（消えた・別のシート）ときは、詳細を開かずに知らせる。
  const notifyRef = useRef(saver.notify);
  useEffect(() => { notifyRef.current = saver.notify; }, [saver.notify]);
  /* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    if (openRow === null || state !== 'normal' || sourceRows.length === 0) return;
    if (sourceRows.some((r) => r.rowNumber === openRow)) return;
    pushedRef.current = false;
    setOpenRow(null);
    writeListUrl({}, 'replace');
    notifyRef.current('その応募者は見つかりませんでした');
  }, [openRow, state, sourceRows]);
  /* eslint-enable react-hooks/set-state-in-effect */

  const openedRow = openRow === null ? undefined : sourceRows.find((r) => r.rowNumber === openRow);
  const urlState = (row: number | null): ListUrlState => ({
    ...(status !== null ? { status } : {}),
    ...(quick !== null ? { quick } : {}),
    ...(debQuery !== '' ? { q: debQuery } : {}),
    ...(sort?.mode === 'near' ? { sort: 'near' as const } : sort?.mode === 'changed' ? { sort: 'changed' as const } : {}),
    ...(row !== null ? { row } : {}),
  });
  // 詳細を開くと履歴に 1 つ積み、戻る操作で詳細だけが閉じる。詳細の中の前後の移動は積まない。
  const open = (rowNumber: number) => {
    actions.markSeen(sheet.id, rowNumber);
    if (openRow === null) {
      scrollRef.current = window.scrollY;
      writeListUrl(urlState(rowNumber), 'push');
      pushedRef.current = true;
    } else {
      writeListUrl(urlState(rowNumber), 'replace');
    }
    setOpenRow(rowNumber);
  };
  const closeRow = () => {
    if (pushedRef.current) {
      pushedRef.current = false;
      window.history.back();
    } else {
      writeListUrl(urlState(null), 'replace');
      setOpenRow(null);
    }
  };

  // 状況タブ・チップ・検索・並びを変えたら URL に書く（履歴は積まない）。
  /* eslint-disable react-hooks/exhaustive-deps -- urlState は上の 4 つから作る */
  useEffect(() => { writeListUrl(urlState(openedRow ? openRow : null), 'replace'); }, [status, quick, debQuery, sortKey]);
  /* eslint-enable react-hooks/exhaustive-deps */

  const exportCsv = () => {
    const stamp = new Date().toISOString().slice(0, 10).replace(/-/g, '');
    downloadText(`${sheet.tabName}_${stamp}.csv`, toCsv(rows, shownCols));
    actions.logExport(sheet.id, rows.length);
    saver.notify(`${String(rows.length)} 人分を書き出しました`);
  };

  // 表の高さを画面の下端までにする（見出しを固定し、横スクロールを表の中だけにするため）。
  const gridWrap = useRef<HTMLDivElement>(null);
  const [gridH, setGridH] = useState(500);
  useEffect(() => {
    const el = gridWrap.current;
    if (!el) return undefined;
    const calc = () => { setGridH(Math.max(240, Math.floor(window.innerHeight - el.getBoundingClientRect().top))); };
    const ro = new ResizeObserver(calc);
    ro.observe(document.documentElement);
    window.addEventListener('resize', calc);
    return () => { ro.disconnect(); window.removeEventListener('resize', calc); };
  }, [isDesktop, state]);

  if (state === 'unavailable') return <Unavailable sheet={sheet} />;
  if (state === 'empty') return <Empty />;

  const readOnly = state === 'stale';
  const statusOptions = statusCol ? [...(statusCol.options ?? []), ...[...statusCounts.keys()].filter((v) => !(statusCol.options ?? []).includes(v))] : [];
  const filterChips = Object.entries(filters).map(([h, f]) => {
    const c = sheet.columns.find((x) => x.header === h);
    const text = f.type === 'in' ? f.values.map((v) => (v === '' ? '（空）' : v)).join('・') : `「${f.text}」を含む`;
    return { h, text: `${c ? labelOf(c) : h}: ${text}` };
  });
  const sortCol = sort ? sheet.columns.find((c) => c.header === sort.column) : undefined;
  const sortLabel = sortCol ? labelOf(sortCol) : (sort?.column ?? '');
  const sortChip = sort === null || sort.mode !== undefined ? null : `並べ替え: ${sortLabel}（${sort.dir === 'asc' ? '昇順' : '降順'}）`;

  const statusTabs = statusCol && (
    <TabScroller fade="var(--color-pt-bg)" label={labelOf(statusCol)}>
      <StatusTab active={status === null} onClick={() => { setStatus(null); }} label="すべて" count={quickRows.length} />
      {statusOptions.filter((o) => o !== '' || (statusCounts.get('') ?? 0) > 0).map((o) => (
        <StatusTab key={o === '' ? '(empty)' : o} active={status === o} onClick={() => { setStatus(status === o ? null : o); }} label={o === '' ? '未入力' : o} count={statusCounts.get(o) ?? 0} dot={statusDot(sheet, o)} />
      ))}
    </TabScroller>
  );

  const editor = openedRow && <RowEditor sheet={sheet} row={openedRow} onClose={closeRow} onMove={open} />;
  const toast = <ToastBar toast={saver.toast} onUndo={() => { saver.closeToast(); saver.undo(); }} onClose={saver.closeToast} />;

  const emptyNote = (quick === 'today' || quick === 'tomorrow') && chips.find((c) => c.id === quick)?.count === 0 ? noInterviewNote(sheet, sourceRows, quick) : null;
  const note = emptyNote !== null && <p className="px-4 pt-2 text-[13px] text-pt-muted lg:px-6">{emptyNote}</p>;
  const narrowed = query !== '' || status !== null || quick !== null || Object.keys(filters).length > 0;
  const clearAll = () => { setQuery(''); setStatus(null); pickQuick(null); changeFilters({}); };
  const emptyActions = rows.length === 0 && sourceRows.length > 0 && narrowed && (
    <div className="flex flex-wrap items-center justify-center gap-2 px-4 pt-6">
      {query !== '' && <Button size="sm" onClick={() => { setQuery(''); }}>検索を消す</Button>}
      <Button size="sm" onClick={clearAll}>絞り込みをすべて外す</Button>
    </div>
  );

  if (!isDesktop) {
    const showToday = sheet.interviewColumn !== undefined && status === null && quick === null && query === '' && Object.keys(filters).length === 0;
    return (
      <div>
        <div className="border-b border-pt-line bg-pt-bg px-3 pt-2 pb-1">
          <TodayBand chips={chips} active={quick} onPick={pickQuick} className="mb-2 -mx-0.5 px-0.5" />
          <div className="flex items-center gap-2">
            <div className="min-w-0 flex-1"><SearchBox value={query} onChange={setQuery} /></div>
            <SortSelect compact sort={viewSort} interviewColumn={sheet.interviewColumn} onChange={changeSort} />
          </div>
          <div className="mt-1 -mx-1">{statusTabs}</div>
        </div>
        {d.viewer.kind !== 'staff' && <HomeScreenHint />}
        {note}
        {showToday && <TodayInterviews sheet={sheet} rows={sourceRows} onOpen={open} onPick={pickQuick} />}
        {emptyActions}
        <div className="h-2" />
        <MobileList sheet={sheet} rows={rows} readOnly={readOnly} saver={saver} onOpen={open} />
        {editor}
        {toast}
      </div>
    );
  }

  return (
    <div>
      <div className="border-b border-pt-line bg-pt-bg px-6 pt-2.5 pb-2">
        <div className="flex flex-wrap items-center gap-2">
          {chips.length > 0 && <TodayBand chips={chips} active={quick} onPick={pickQuick} className="shrink-0" />}
          <div className="w-[200px]"><SearchBox value={query} onChange={setQuery} /></div>
          <SortSelect sort={viewSort} interviewColumn={sheet.interviewColumn} onChange={changeSort} className="w-[150px] shrink-0" />
          {(filterChips.length > 0 || sortChip) && (
            <div className="flex min-w-0 flex-wrap items-center gap-1">
              {filterChips.map((f) => <Chip key={f.h} onRemove={() => { changeFilters(omit(filters, f.h)); }}>{f.text}</Chip>)}
              {sortChip && <Chip onRemove={() => { changeSort(null); }}>{sortChip}</Chip>}
            </div>
          )}
          <span className="ml-auto" />
          <ColumnPicker sheet={sheet} statusHeader={statusCol?.header} titleHeader={titleHeader} override={shownOverride} onChange={changeShown} />
          <Button size="sm" onClick={exportCsv}><IconDownload size={14} />書き出し</Button>
        </div>
        <div className="mt-2 -ml-1">{statusTabs}</div>
      </div>
      {note}
      {emptyActions}
      <div ref={gridWrap}>
        <Grid
          sheet={sheet}
          rows={rows}
          allRows={sourceRows}
          cols={shownCols}
          titleHeader={titleHeader}
          readOnly={readOnly}
          saver={saver}
          sort={viewSort}
          onSort={changeSort}
          filters={filters}
          onFilter={(h, f) => { changeFilters(f ? { ...filters, [h]: f } : omit(filters, h)); }}
          onOpen={open}
          openRowNumber={openRow}
          canEditColumns={canEditColumns}
          onColumnSettings={(h) => {
            try { sessionStorage.setItem('pt.columns.pick', JSON.stringify({ sheetId: sheet.id, header: h })); } catch { /* 保存できなくても移動はする */ }
            nav.go(ROUTES.columns);
          }}
          jump={jump}
          height={gridH}
        />
      </div>
      {editor}
      {toast}
    </div>
  );
}

function SearchBox({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <label className="relative block">
      <span className="sr-only">検索</span>
      <IconSearch size={14} className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-pt-faint" />
      <input
        type="search"
        value={value}
        onChange={(e) => { onChange(e.target.value); }}
        placeholder="氏名・ふりがな・電話"
        className="h-[40px] w-full rounded-md border border-pt-line-strong bg-pt-surface pr-2 pl-8 text-base text-pt-ink placeholder:text-pt-faint focus:border-pt-select focus:ring-2 focus:ring-pt-select/20 focus:outline-none lg:h-8 lg:text-[13px]"
      />
    </label>
  );
}

function StatusTab({ active, onClick, label, count, dot }: { active: boolean; onClick: () => void; label: string; count: number; dot?: string }) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cx('inline-flex h-[44px] min-h-[44px] shrink-0 items-center gap-1.5 rounded-md px-3 text-[13px] whitespace-nowrap lg:h-7 lg:min-h-0 lg:gap-1 lg:px-1.5 lg:text-[12px]', active ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card ring-1 ring-pt-line' : 'text-pt-muted hover:bg-pt-sunken hover:text-pt-ink')}
    >
      {dot && <span className={cx('h-1.5 w-1.5 rounded-full', dot)} />}
      {label}
      <span className={cx('pt-num text-[12px]', active ? 'text-pt-muted' : 'text-pt-faint')}>{count}</span>
    </button>
  );
}

function Chip({ children, onRemove }: { children: ReactNode; onRemove: () => void }) {
  return (
    <span className="inline-flex h-7 max-w-72 items-center gap-1 rounded-md border border-pt-line bg-pt-surface pr-1 pl-2 text-[12px] text-pt-ink">
      <span className="truncate">{children}</span>
      <button type="button" aria-label="外す" onClick={onRemove} className="rounded p-0.5 text-pt-faint hover:bg-pt-sunken hover:text-pt-ink"><IconX size={12} /></button>
    </span>
  );
}

function ColumnPicker({ sheet, statusHeader, titleHeader, override, onChange }: {
  sheet: SheetConfig;
  statusHeader: string | undefined;
  titleHeader: string;
  override: Record<string, boolean>;
  onChange: (o: Record<string, boolean>) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useOutside(ref, () => { setOpen(false); }, open);
  const cols = orderColumns(sheet.columns.filter((c) => c.visible), titleHeader, statusHeader);
  const shown = (h: string, inList: boolean) => override[h] ?? inList;
  const hiddenCount = cols.filter((c) => c.header !== titleHeader && !shown(c.header, c.inList)).length;
  return (
    <div ref={ref} className="relative">
      <Button size="sm" onClick={() => { setOpen(!open); }} aria-expanded={open}>
        <IconColumns size={14} />列の表示{hiddenCount > 0 && <span className="pt-num text-[12px] text-pt-faint">{hiddenCount} 列を非表示</span>}
      </Button>
      {open && (
        <div className="absolute top-full right-0 z-40 mt-1 w-64 rounded-lg border border-pt-line bg-pt-surface py-1 shadow-pt-pop">
          <div className="max-h-[min(560px,calc(100vh-180px))] overflow-y-auto">
            {cols.map((c) => {
              const isTitle = c.header === titleHeader;
              return (
                <label key={c.header} className={cx('flex h-8 items-center gap-2 px-3 text-[13px]', isTitle ? 'text-pt-faint' : 'cursor-pointer hover:bg-pt-sunken')}>
                  <input type="checkbox" disabled={isTitle} checked={isTitle || shown(c.header, c.inList)} onChange={(e) => { onChange({ ...override, [c.header]: e.target.checked }); }} className="h-3.5 w-3.5 accent-pt-select" />
                  <span className="truncate">{labelOf(c)}</span>
                  {isTitle && <span className="ml-auto text-[12px]">常に表示</span>}
                </label>
              );
            })}
          </div>
          <div className="mt-1 flex gap-1 border-t border-pt-line px-2 pt-1">
            <button type="button" onClick={() => { onChange(Object.fromEntries(cols.map((c) => [c.header, true]))); }} className="h-7 rounded px-2 text-[12px] text-pt-primary hover:bg-pt-primary-soft">すべて表示</button>
            <button type="button" onClick={() => { onChange({}); }} className="h-7 rounded px-2 text-[12px] text-pt-muted hover:bg-pt-sunken">最初の表示に戻す</button>
          </div>
        </div>
      )}
    </div>
  );
}


/** スマホで初めて開いたときの、ホーム画面に追加する案内。閉じたら覚えて、出さない。 */
function HomeScreenHint() {
  const [hidden, setHidden] = useState(() => {
    try { return localStorage.getItem('pt.a2hs.dismissed') === '1' || window.matchMedia('(display-mode: standalone)').matches; } catch { return false; }
  });
  const [how, setHow] = useState(false);
  const closeHow = () => { setHow(false); };
  if (hidden) return null;
  const dismiss = () => {
    try { localStorage.setItem('pt.a2hs.dismissed', '1'); } catch { /* 覚えられなくても閉じる */ }
    setHidden(true);
  };
  return (
    <>
      <div className="mx-3 mt-2 flex items-center gap-1 rounded-[8px] border border-pt-line bg-pt-surface pl-3.5 text-[13px] text-pt-ink">
        <span className="min-w-0 flex-1 py-2">ホーム画面に追加すると、次からすぐ開けます</span>
        <button type="button" onClick={() => { setHow(true); }} className="inline-flex h-[44px] shrink-0 items-center px-2 font-medium text-pt-primary">やり方</button>
        <button type="button" aria-label="この案内を閉じる" onClick={dismiss} className="inline-flex h-[44px] w-[44px] shrink-0 items-center justify-center text-pt-faint hover:text-pt-ink"><IconX size={14} /></button>
      </div>
      <BottomSheet open={how} title="ホーム画面に追加する" onClose={closeHow}>
        <div className="space-y-4 px-4 pb-4 text-[15px]">
          <div>
            <p className="font-semibold">iPhone（Safari）</p>
            <p className="mt-1 text-pt-muted">画面下の共有ボタンを押し、「ホーム画面に追加」を選びます。</p>
          </div>
          <div>
            <p className="font-semibold">Android（Chrome）</p>
            <p className="mt-1 text-pt-muted">右上のメニューを押し、「ホーム画面に追加」を選びます。</p>
          </div>
        </div>
      </BottomSheet>
    </>
  );
}
