// ログイン後の見る画面（計画の PR4）。自分の顧客のシートの応募者を、PC は表、スマホはカードで出す。読み取りだけ。
// 画面を開いている間（タブが見えている間）だけ、30 秒おきに軽い確認（版）を呼び、変わったときだけ読み直す。
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { PortalApplicant } from '../../../../generated/PortalApplicant';
import type { PortalColumn } from '../../../../generated/PortalColumn';
import type { PortalMe } from '../../../../generated/PortalMe';
import type { PortalSheetRows } from '../../../../generated/PortalSheetRows';
import type { PortalSheets } from '../../../../generated/PortalSheets';
import type { PortalSheetVersion } from '../../../../generated/PortalSheetVersion';
import { IconChevronDown, IconChevronRight, IconLogout, IconMail, IconPhone, IconSearch, IconSheet, IconX } from '../../ui/icons';
import { Badge, Button, cx, formatTime, IconButton, Notice } from '../../ui/kit';
import { call, NETWORK } from '../api';
import { DETAIL_GROUPS, GROUP_TITLE, groupOf, listLayout, mailHref, matches, newestFirst, readUrl, statusTone, telHref, urlFor, type ListLayout } from './model';

const CHECK_EVERY_MS = 30_000;
const PATH = '/portal';

/** 読み込みの状態。どのシートの結果かを持ち、表示中のシートと違えば「読み込み中」として扱う。 */
type Load =
  | { sheet: string | null; state: 'loading' }
  | { sheet: string; state: 'ready'; data: PortalSheetRows }
  | { sheet: string; state: 'failed'; message: string };

function message(r: { error: string; message: string }): string {
  if (r.error === NETWORK) return '通信できませんでした。電波のよい所で、もう一度お試しください';
  return r.message || 'うまくいきませんでした。少し待ってからもう一度お試しください';
}

function StatusBadge({ value }: { value: string }) {
  if (value.trim() === '') return <span className="text-pt-faint">未入力</span>;
  return <Badge tone={statusTone(value)}>{value}</Badge>;
}

/** 1 つの値。電話・メールは押せるようにし、長い文章は改行を残す。 */
function Value({ column, value }: { column: PortalColumn; value: string }) {
  if (value.trim() === '') return <span className="text-pt-faint">—</span>;
  if (column.kind === 'phone') {
    const href = telHref(value);
    if (href) return <a href={href} className="pt-num inline-flex items-center gap-1 text-pt-primary hover:underline"><IconPhone size={13} />{value}</a>;
  }
  if (column.kind === 'email') {
    const href = mailHref(value);
    if (href) return <a href={href} className="inline-flex min-w-0 items-center gap-1 break-all text-pt-primary hover:underline"><IconMail size={13} className="shrink-0" />{value}</a>;
  }
  if (column.kind === 'choice') return <StatusBadge value={value} />;
  const num = column.kind === 'date' || column.kind === 'datetime';
  return <span className={cx(column.multiline ? 'whitespace-pre-wrap break-words' : 'break-words', num && 'pt-num')}>{value}</span>;
}

/** 更新の状態（見出しの横に 1 か所）。 */
function SyncLine({ data, problem, onReload }: { data: PortalSheetRows; problem: boolean; onReload: () => void }) {
  if (data.stale || problem) {
    return (
      <span className="inline-flex flex-wrap items-center gap-x-1.5 text-xs text-pt-warn">
        <span className="h-1.5 w-1.5 rounded-full bg-pt-warn" />
        <span><span className="pt-num">{formatTime(data.read_at)}</span> 時点の内容です。シートを確かめられませんでした</span>
        <button type="button" onClick={onReload} className="font-medium text-pt-primary hover:underline">読み直す</button>
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap text-pt-faint" title="画面を開いている間は、30 秒おきにシートを確かめます">
      <span className="h-1.5 w-1.5 rounded-full bg-pt-ok" />
      <span className="pt-num">{formatTime(data.read_at)}</span> 時点
    </span>
  );
}

function SheetSwitch({ sheets, current, onSelect }: { sheets: PortalSheets['sheets']; current: string | null; onSelect: (id: string) => void }) {
  if (sheets.length <= 1) {
    const only = sheets[0];
    return only ? <span className="inline-flex min-w-0 items-center gap-1.5 truncate text-[13px] text-pt-muted"><IconSheet size={14} className="shrink-0 text-pt-faint" />{only.label}</span> : null;
  }
  return (
    <>
      <label className="relative flex h-[40px] min-w-0 flex-1 items-center lg:hidden">
        <span className="sr-only">シート</span>
        <IconSheet size={14} className="pointer-events-none absolute left-2.5 text-pt-faint" />
        <select value={current ?? ''} onChange={(e) => { onSelect(e.target.value); }} className="h-[40px] w-full min-w-0 appearance-none truncate rounded-[6px] border border-pt-line-strong bg-pt-surface pr-8 pl-8 text-base text-pt-ink">
          {sheets.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
        </select>
        <IconChevronDown size={14} className="pointer-events-none absolute right-2.5 text-pt-faint" />
      </label>
      <div className="hidden min-w-0 items-center gap-0.5 overflow-x-auto rounded-md bg-pt-sunken p-0.5 lg:flex" role="tablist" aria-label="シート">
        {sheets.map((s) => (
          <button
            key={s.id}
            type="button"
            role="tab"
            aria-selected={s.id === current}
            onClick={() => { onSelect(s.id); }}
            className={cx('inline-flex h-7 items-center gap-1.5 rounded px-2.5 text-[13px] whitespace-nowrap', s.id === current ? 'bg-pt-surface font-medium text-pt-ink shadow-pt-card' : 'text-pt-muted hover:text-pt-ink')}
          >
            <IconSheet size={13} className={s.id === current ? 'text-pt-muted' : 'text-pt-faint'} />{s.label}
          </button>
        ))}
      </div>
    </>
  );
}

function Table({ data, layout, rows, selected, onOpen }: { data: PortalSheetRows; layout: ListLayout; rows: PortalApplicant[]; selected: number | null; onOpen: (row: number) => void }) {
  return (
    <div className="overflow-x-auto rounded-lg border border-pt-line bg-pt-surface shadow-pt-card">
      <table className="pt-grid w-full border-collapse text-[13px]">
        <thead>
          <tr className="border-b border-pt-line bg-pt-bg text-left text-xs text-pt-muted">
            {layout.list.map((i) => <th key={i} scope="col" className="px-3 py-2 font-medium whitespace-nowrap">{data.columns[i]?.label}</th>)}
            <th scope="col" className="w-8"><span className="sr-only">詳細</span></th>
          </tr>
        </thead>
        <tbody>
          {rows.map((a) => {
            const name = layout.title !== null ? (a.cells[layout.title] ?? '') : '';
            return (
              <tr
                key={a.row}
                onClick={() => { onOpen(a.row); }}
                aria-selected={a.row === selected}
                className={cx('cursor-pointer border-b border-pt-line last:border-b-0 hover:bg-pt-bg', a.row === selected && 'bg-pt-primary-soft hover:bg-pt-primary-soft')}
              >
                {layout.list.map((i) => {
                  const c = data.columns[i];
                  const v = a.cells[i] ?? '';
                  if (!c) return <td key={i} />;
                  if (i === layout.title) {
                    return (
                      <td key={i} className="px-3 py-2.5 font-medium whitespace-nowrap text-pt-ink">
                        <button type="button" onClick={(e) => { e.stopPropagation(); onOpen(a.row); }} className="text-left hover:underline">{v === '' ? '（名前なし）' : v}</button>
                      </td>
                    );
                  }
                  return (
                    <td key={i} className={cx('max-w-[280px] truncate px-3 py-2.5 text-pt-ink', (c.kind === 'date' || c.kind === 'datetime' || c.kind === 'phone') && 'pt-num whitespace-nowrap')} title={c.kind === 'choice' ? undefined : v}>
                      {c.kind === 'choice' ? <StatusBadge value={v} /> : v === '' ? <span className="text-pt-faint">—</span> : v}
                    </td>
                  );
                })}
                <td className="pr-2 text-pt-faint" aria-hidden="true"><IconChevronRight size={14} /><span className="sr-only">{name}</span></td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Cards({ data, layout, rows, onOpen }: { data: PortalSheetRows; layout: ListLayout; rows: PortalApplicant[]; onOpen: (row: number) => void }) {
  return (
    <ul className="flex flex-col gap-2">
      {rows.map((a) => {
        const name = layout.title !== null ? (a.cells[layout.title] ?? '') : '';
        const status = layout.status !== null ? (a.cells[layout.status] ?? '') : null;
        const sub = layout.sub.map((i) => a.cells[i] ?? '').filter((v) => v.trim() !== '').join(' / ');
        return (
          <li key={a.row}>
            <button type="button" onClick={() => { onOpen(a.row); }} className="block w-full rounded-[8px] border border-pt-line bg-pt-surface px-[14px] py-3 text-left shadow-pt-card active:bg-pt-bg">
              <span className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-base font-semibold text-pt-ink">{name === '' ? '（名前なし）' : name}</span>
                {status !== null && <span className="shrink-0"><StatusBadge value={status} /></span>}
              </span>
              {sub !== '' && <span className="pt-num mt-1 block truncate text-[13px] text-pt-muted">{sub}</span>}
            </button>
          </li>
        );
      })}
      {data.applicants.length > 0 && <li className="pt-2 text-center text-xs text-pt-faint">新しく入った応募者から並べています</li>}
    </ul>
  );
}

function Detail({ data, layout, applicant, onClose }: { data: PortalSheetRows; layout: ListLayout; applicant: PortalApplicant; onClose: () => void }) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { closeRef.current?.focus(); }, [applicant.row]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', k);
    return () => { document.removeEventListener('keydown', k); };
  }, [onClose]);
  const name = layout.title !== null ? (applicant.cells[layout.title] ?? '') : '';
  const status = layout.status !== null ? (applicant.cells[layout.status] ?? '') : null;
  const fields = (pick: (c: PortalColumn, i: number) => boolean) => data.columns.map((c, i) => ({ c, i })).filter(({ c, i }) => pick(c, i));
  const summary = fields((c, i) => groupOf(c) === 'summary' && i !== layout.title && i !== layout.status);
  const rowsOf = (list: { c: PortalColumn; i: number }[]): ReactNode => (
    <dl className="grid grid-cols-[8.5em_1fr] gap-x-3 gap-y-2.5 text-sm">
      {list.map(({ c, i }) => (
        <div key={i} className="contents">
          <dt className="pt-0.5 text-[13px] text-pt-muted">{c.label}</dt>
          <dd className="min-w-0 text-pt-ink"><Value column={c} value={applicant.cells[i] ?? ''} /></dd>
        </div>
      ))}
    </dl>
  );
  return (
    <div role="dialog" aria-modal="true" aria-label={`${name}さんの詳細`} className="fixed inset-0 z-[60] flex justify-end bg-black/30 lg:bg-black/10" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="pt-sheet-enter flex h-full w-full flex-col bg-pt-surface shadow-pt-pop lg:max-w-[460px]">
        <div className="flex items-center gap-2 border-b border-pt-line px-4 py-2.5">
          <button ref={closeRef} type="button" onClick={onClose} className="-ml-2 inline-flex h-[44px] items-center gap-1 rounded-md px-2 text-[15px] text-pt-muted hover:bg-pt-sunken lg:hidden">
            <span aria-hidden="true">←</span>一覧へ
          </button>
          <span className="hidden text-xs text-pt-faint lg:inline">応募者の詳細</span>
          <span className="ml-auto hidden lg:block"><IconButton label="閉じる" onClick={onClose}><IconX size={16} /></IconButton></span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pt-4 pb-[calc(24px+env(safe-area-inset-bottom))]">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="min-w-0 text-[19px] font-semibold break-words text-pt-ink">{name === '' ? '（名前なし）' : name}</h2>
            {status !== null && <StatusBadge value={status} />}
          </div>
          {summary.length > 0 && <div className="mt-4">{rowsOf(summary)}</div>}
          {DETAIL_GROUPS.map((g) => {
            const list = fields((c, i) => groupOf(c) === g && i !== layout.title && i !== layout.status);
            if (list.length === 0) return null;
            return (
              <section key={g} className="mt-6">
                <h3 className="mb-2.5 border-b border-pt-line pb-1.5 text-[13px] font-semibold text-pt-ink">{GROUP_TITLE[g]}</h3>
                {rowsOf(list)}
              </section>
            );
          })}
          <p className="mt-8 text-xs text-pt-faint">シートの {applicant.row} 行目の内容です。</p>
        </div>
      </div>
    </div>
  );
}

export function ApplicantsScreen({ me, onSignedOut, onLogout }: { me: PortalMe; onSignedOut: () => void; onLogout: () => void }) {
  const [sheets, setSheets] = useState<PortalSheets['sheets'] | null>(null);
  const [listProblem, setListProblem] = useState<string | null>(null);
  const [url, setUrl] = useState(() => readUrl(window.location.search));
  const [rawLoad, setLoad] = useState<Load>({ sheet: null, state: 'loading' });
  const [checkProblem, setCheckProblem] = useState(false);
  const [query, setQuery] = useState('');
  const versionRef = useRef<string | null>(null);
  const reqRef = useRef(0);

  const sheetId = sheets && sheets.length > 0 ? (sheets.find((s) => s.id === url.sheet)?.id ?? sheets[0]?.id ?? null) : null;

  useEffect(() => {
    void call<PortalSheets>('GET', '/api/portal/sheets').then((r) => {
      if (r.ok) setSheets(r.data.sheets);
      else if (r.status === 401) onSignedOut();
      else setListProblem(message(r));
    });
  }, [onSignedOut]);

  useEffect(() => {
    const onPop = () => { setUrl(readUrl(window.location.search)); };
    window.addEventListener('popstate', onPop);
    return () => { window.removeEventListener('popstate', onPop); };
  }, []);

  /** 読み直す。quiet なら失敗しても今の表示を残し、帯で知らせる */
  const reload = useCallback((id: string, quiet: boolean) => {
    const req = ++reqRef.current;
    void call<PortalSheetRows>('GET', `/api/portal/sheets/${encodeURIComponent(id)}/rows`).then((r) => {
      if (req !== reqRef.current) return;
      if (r.ok) {
        versionRef.current = r.data.version;
        setCheckProblem(false);
        setLoad({ sheet: id, state: 'ready', data: r.data });
      } else if (r.status === 401) {
        onSignedOut();
      } else if (quiet) {
        setCheckProblem(true);
      } else {
        setLoad({ sheet: id, state: 'failed', message: message(r) });
      }
    });
  }, [onSignedOut]);

  useEffect(() => {
    if (!sheetId) return;
    versionRef.current = null;
    reload(sheetId, false);
  }, [sheetId, reload]);

  // 表示中だけ 30 秒おきに版を確かめる。タブが隠れている間は止め、戻ったらすぐ確かめる
  useEffect(() => {
    if (!sheetId) return undefined;
    let timer: number | undefined;
    const check = () => {
      if (document.hidden || versionRef.current === null) return;
      void call<PortalSheetVersion>('GET', `/api/portal/sheets/${encodeURIComponent(sheetId)}/version`).then((r) => {
        if (r.ok) {
          if (r.data.version !== versionRef.current) reload(sheetId, true);
          else setCheckProblem(false);
        } else if (r.status === 401) {
          onSignedOut();
        } else {
          setCheckProblem(true);
        }
      });
    };
    const start = () => { window.clearInterval(timer); timer = window.setInterval(check, CHECK_EVERY_MS); };
    const onVisible = () => {
      if (document.hidden) { window.clearInterval(timer); return; }
      check();
      start();
    };
    if (!document.hidden) start();
    document.addEventListener('visibilitychange', onVisible);
    return () => { window.clearInterval(timer); document.removeEventListener('visibilitychange', onVisible); };
  }, [sheetId, reload, onSignedOut]);

  const go = (sheet: string | null, row: number | null, push: boolean) => {
    const next = urlFor(PATH, sheet, row);
    if (push) window.history.pushState(null, '', next);
    else window.history.replaceState(null, '', next);
    setUrl({ sheet, row });
  };

  const load: Load = rawLoad.sheet === sheetId ? rawLoad : { sheet: sheetId, state: 'loading' };
  const data = load.state === 'ready' ? load.data : null;
  const layout = useMemo(() => (data ? listLayout(data.columns) : null), [data]);
  const shown = useMemo(() => (data ? newestFirst(data.applicants.filter((a) => matches(a, query))) : []), [data, query]);
  const open = data && url.row !== null ? data.applicants.find((a) => a.row === url.row) : undefined;
  const closeDetail = useCallback(() => {
    // 詳細を開いたときに履歴を 1 つ積んでいるので、戻るで閉じる (直接開いた URL なら置き換える)
    if (window.history.state === 'portal-detail') window.history.back();
    else { window.history.replaceState(null, '', urlFor(PATH, sheetId, null)); setUrl({ sheet: sheetId, row: null }); }
  }, [sheetId]);
  const openRow = (row: number) => {
    window.history.pushState('portal-detail', '', urlFor(PATH, sheetId, row));
    setUrl({ sheet: sheetId, row });
  };

  let body: ReactNode;
  if (listProblem) body = <Notice tone="warn">{listProblem}</Notice>;
  else if (!sheets) body = <p className="py-12 text-center text-[13px] text-pt-muted">読み込んでいます…</p>;
  else if (sheets.length === 0) body = <Notice tone="info" title="見られるシートがまだありません">準備ができたら、弊社の担当者からご案内します。</Notice>;
  else if (load.state === 'loading') body = <p className="py-12 text-center text-[13px] text-pt-muted">応募者を読み込んでいます…</p>;
  else if (load.state === 'failed') body = <Notice tone="warn" action={sheetId ? <Button size="sm" onClick={() => { setLoad({ sheet: sheetId, state: 'loading' }); reload(sheetId, false); }}>読み直す</Button> : undefined}>{load.message}</Notice>;
  else if (data && layout) {
    body = (
      <>
        <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-2">
          <label className="relative flex w-full items-center sm:w-[280px]">
            <span className="sr-only">応募者を探す</span>
            <IconSearch size={14} className="pointer-events-none absolute left-3 text-pt-faint" />
            <input type="search" value={query} onChange={(e) => { setQuery(e.target.value); }} placeholder="名前・電話番号などで探す" className="h-[40px] w-full rounded-md border border-pt-line-strong bg-pt-surface pr-3 pl-8 text-base text-pt-ink placeholder:text-pt-faint focus:border-pt-primary focus:ring-2 focus:ring-pt-primary-soft focus:outline-none sm:h-9 sm:text-[13px]" />
          </label>
          <span className="pt-num text-[13px] text-pt-muted">{query.trim() !== '' ? `${String(shown.length)} 人 / ${String(data.applicants.length)} 人` : `${String(data.applicants.length)} 人`}</span>
          <span className="ml-auto"><SyncLine data={data} problem={checkProblem} onReload={() => { if (sheetId) reload(sheetId, true); }} /></span>
        </div>
        {data.applicants.length === 0 ? (
          <p className="rounded-lg border border-pt-line bg-pt-surface px-4 py-12 text-center text-[13px] text-pt-muted">まだ応募者はいません</p>
        ) : shown.length === 0 ? (
          <p className="rounded-lg border border-pt-line bg-pt-surface px-4 py-12 text-center text-[13px] text-pt-muted">条件に合う応募者はいません</p>
        ) : (
          <>
            <div className="hidden lg:block"><Table data={data} layout={layout} rows={shown} selected={open?.row ?? null} onOpen={openRow} /></div>
            <div className="lg:hidden"><Cards data={data} layout={layout} rows={shown} onOpen={openRow} /></div>
          </>
        )}
        {open && <Detail data={data} layout={layout} applicant={open} onClose={closeDetail} />}
      </>
    );
  }

  return (
    <div className="min-h-dvh bg-pt-bg">
      <header className="sticky top-0 z-40 border-b border-pt-line bg-pt-surface">
        <div className="mx-auto flex h-[52px] max-w-[1280px] items-center gap-3 px-4 lg:gap-5">
          <span className="inline-flex shrink-0 items-center gap-2 text-[13px] font-semibold text-pt-ink">
            <span className="flex size-6 items-center justify-center rounded bg-pt-brand text-white"><IconSheet size={13} /></span>
            <span className="hidden sm:inline">応募者の管理</span>
          </span>
          {sheets && <div className="flex min-w-0 flex-1 items-center"><SheetSwitch sheets={sheets} current={sheetId} onSelect={(id) => { go(id, null, false); setQuery(''); }} /></div>}
          <span className="ml-auto hidden max-w-[240px] truncate text-xs text-pt-muted md:inline" title={me.email}>{me.email}</span>
          <IconButton label="ログアウト" onClick={onLogout} className="ml-auto md:ml-0"><IconLogout size={16} /></IconButton>
        </div>
      </header>
      <main className="mx-auto max-w-[1280px] px-4 py-4 lg:py-5">{body}</main>
    </div>
  );
}
