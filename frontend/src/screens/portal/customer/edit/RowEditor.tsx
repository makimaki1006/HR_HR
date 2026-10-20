// 応募者 1 人の詳細。項目を「項目名｜値」で並べ、書ける項目は値を押すとその場で直せる。担当: detail チーム。
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { actions, cellHistory, getData, log, useData } from '../../data';
import { cardLayout, groupOf, kindOf, labelOf } from '../../columns';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { Badge, cx, formatShortDateTime } from '../../ui/kit';
import { CellDisplay, CellInput } from '../../ui/cell';
import { IconCheck, IconChevronDown, IconLock, IconMail, IconPhone, IconX } from '../../ui/icons';
import { statusDot } from '../list/model';
import { ChoicePopover } from './ChoicePopover';
import { IconPencil } from './icons';
import { Timeline } from './Timeline';
import { useVisibleRows } from './visibleRows';

export interface RowEditorProps {
  sheet: SheetConfig;
  row: ApplicantRow;
  onClose: () => void;
  /** 前後の応募者へ移ったとき（一覧で開いている行の色を合わせる）。 */
  onMove?: (rowNumber: number) => void;
}

type CellState =
  | { kind: 'saving' }
  | { kind: 'saved'; undo?: { prev: string; cur: string } }
  | { kind: 'conflict'; latest: string; by: string; at: string; mine: string }
  | { kind: 'failed'; mine: string };

interface Editing {
  column: string;
  base: string;
  draft: string;
}

const shown = (v: string): string => (v === '' ? '（空）' : v);

function isTyping(t: EventTarget | null): boolean {
  return t instanceof HTMLElement && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT');
}

const hasOptions = (c: ColumnConfig): boolean => c.options !== null && c.options.length > 0;

function NavButton({ label, disabled, onClick, children }: { label: string; disabled?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" aria-label={label} title={label} disabled={disabled} onClick={onClick} className="inline-flex h-[44px] w-[44px] shrink-0 items-center justify-center rounded-md text-pt-muted hover:bg-pt-sunken hover:text-pt-ink focus-visible:outline-2 focus-visible:outline-pt-select disabled:opacity-40 sm:h-8 sm:w-8">
      {children}
    </button>
  );
}

function StatusPill({ value, dot }: { value: string; dot: string }) {
  if (value === '') return <span className="text-[13px] text-pt-faint">状況を選ぶ</span>;
  return (
    <span className="inline-flex h-[22px] max-w-full items-center gap-1.5 rounded-full border border-pt-line bg-pt-surface pr-2 pl-1.5 text-[12px] text-pt-ink">
      <span className={cx('h-1.5 w-1.5 shrink-0 rounded-full', dot)} />
      <span className="truncate">{value}</span>
    </span>
  );
}

function RowDetail({ sheet, rowNumber, fallback, order, onClose, onNavigate }: {
  sheet: SheetConfig;
  rowNumber: number;
  fallback: ApplicantRow;
  order: number[] | undefined;
  onClose: () => void;
  onNavigate: (rowNumber: number) => void;
}) {
  const data = useData();
  const rows = useMemo(() => data.rows[sheet.id] ?? [], [data.rows, sheet.id]);
  const found = rows.find((r) => r.rowNumber === rowNumber);
  const live = found ?? fallback;
  const values = live.values;
  const [baseline, setBaseline] = useState<Record<string, string>>(() => ({ ...live.values }));
  const [editing, setEditing] = useState<Editing | null>(null);
  const editingRef = useRef<Editing | null>(null);
  const [cells, setCells] = useState<Record<string, CellState>>({});
  const [choiceOpen, setChoiceOpen] = useState<string | null>(null);
  const [showOther, setShowOther] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  const viewed = useRef(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const editWrapRef = useRef<HTMLDivElement>(null);

  const layout = cardLayout(sheet);
  const name = layout.title ? (values[layout.title.header] ?? '') : '';

  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    actions.markSeen(sheet.id, rowNumber);
    log({ kind: 'view', detail: `${name}さんの詳細を表示`, sheetId: sheet.id, rowNumber });
  }, [sheet.id, rowNumber, name]);

  useEffect(() => {
    if (window.matchMedia('(min-width: 640px)').matches) panelRef.current?.querySelector<HTMLElement>('[data-field]')?.focus();
  }, []);

  // 前後の移動は、一覧で見えている並びがあればそれに、なければシートの行の順に従う。
  const sequence = useMemo(() => {
    const existing = new Set(rows.map((r) => r.rowNumber));
    const fromList = (order ?? []).filter((n) => existing.has(n));
    return fromList.includes(rowNumber) ? fromList : rows.map((r) => r.rowNumber);
  }, [order, rows, rowNumber]);
  const index = sequence.indexOf(rowNumber);
  const prev = index > 0 ? sequence[index - 1] : undefined;
  const next = index >= 0 ? sequence[index + 1] : undefined;

  const setEd = (e: Editing | null) => { editingRef.current = e; setEditing(e); };
  const valueNow = (column: string): string => getData().rows[sheet.id]?.find((r) => r.rowNumber === rowNumber)?.values[column] ?? '';
  const clearCell = (column: string) => { setCells((c) => Object.fromEntries(Object.entries(c).filter(([k]) => k !== column))); };
  const focusField = (column: string) => {
    setTimeout(() => { panelRef.current?.querySelector<HTMLElement>(`[data-field="${CSS.escape(column)}"]`)?.focus(); }, 0);
  };

  const run = async (column: string, base: string, value: string, withUndo = false) => {
    setCells((c) => ({ ...c, [column]: { kind: 'saving' } }));
    const res = await actions.saveCell(sheet.id, rowNumber, column, base, value);
    const done = (v: string) => {
      setBaseline((b) => ({ ...b, [column]: v }));
      setCells((c) => ({ ...c, [column]: withUndo ? { kind: 'saved', undo: { prev: base, cur: v } } : { kind: 'saved' } }));
      setTimeout(() => { setCells((c) => (c[column]?.kind === 'saved' ? Object.fromEntries(Object.entries(c).filter(([k]) => k !== column)) : c)); }, withUndo ? 5000 : 3000);
    };
    if (res.kind === 'saved') done(res.value);
    else if (res.kind === 'conflict') {
      if (res.latest === value) done(value);
      else setCells((c) => ({ ...c, [column]: { kind: 'conflict', latest: res.latest, by: res.by, at: res.at, mine: value } }));
    } else {
      setCells((c) => ({ ...c, [column]: { kind: 'failed', mine: value } }));
      const col = sheet.columns.find((x) => x.header === column);
      if (col && !hasOptions(col)) setEd({ column, base, draft: value });
    }
  };

  const commitEditing = () => {
    const e = editingRef.current;
    if (!e) return;
    setEd(null);
    if (e.draft !== e.base) void run(e.column, e.base, e.draft);
  };

  const startEdit = (column: string) => {
    commitEditing();
    const v = valueNow(column);
    clearCell(column);
    setBaseline((b) => ({ ...b, [column]: v }));
    setChoiceOpen(null);
    setEd({ column, base: v, draft: v });
  };

  const requestClose = () => { commitEditing(); onClose(); };
  const go = (n: number | undefined) => { if (n === undefined) return; commitEditing(); onNavigate(n); };

  // 欄の外を押したら、いまの入力をそのまま保存する（Esc だけが取り消し）。
  useEffect(() => {
    if (!editing) return undefined;
    const h = (e: MouseEvent) => {
      if (editWrapRef.current && e.target instanceof Node && !editWrapRef.current.contains(e.target)) commitEditing();
    };
    document.addEventListener('mousedown', h);
    return () => { document.removeEventListener('mousedown', h); };
  });

  const visible = useMemo(() => sheet.columns.filter((c) => c.visible), [sheet.columns]);
  const phoneCol = visible.find((c) => kindOf(c) === 'phone');
  const mailCol = visible.find((c) => kindOf(c) === 'email');
  const phone = phoneCol ? (values[phoneCol.header] ?? '') : '';
  const mail = mailCol ? (values[mailCol.header] ?? '') : '';
  const statusCol = layout.status;
  const statusHeader = statusCol?.header;
  const statusValue = statusHeader ? (values[statusHeader] ?? '') : '';
  const titleCol = layout.title;
  const isContactOnly = (c: ColumnConfig) => (c === phoneCol || c === mailCol) && !c.writable;
  const metaCols = visible.filter((c) => groupOf(c) === 'summary' && c !== titleCol && c !== phoneCol && c !== mailCol && c !== statusCol);
  const meta = metaCols.map((c) => {
    const v = values[c.header] ?? '';
    if (v === '') return '';
    const k = kindOf(c);
    if (k === 'date' || k === 'datetime') {
      const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})/.exec(v);
      const t = m ? `${m[2] ?? ''}/${m[3] ?? ''}` : v;
      return c.header === sheet.sortColumn ? `${t} 応募` : t;
    }
    if (c.header.includes('年齢') && /^\d+$/.test(v)) return `${v}歳`;
    return v;
  }).filter((x) => x !== '');

  const sections = useMemo(() => {
    const pick = (g: ReturnType<typeof groupOf>) => visible.filter((c) => groupOf(c) === g && c !== titleCol && c !== statusCol && !isContactOnly(c));
    return [
      { title: '貴社で記入', cols: pick('customer') },
      { title: '弊社からの連絡', cols: pick('staff') },
      { title: '応募の内容', cols: pick('summary') },
    ].filter((s) => s.cols.length > 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, titleCol, statusCol, phoneCol, mailCol]);
  const other = visible.filter((c) => groupOf(c) === 'other' && c !== statusCol && !isContactOnly(c));

  const fieldOrder = [...sections.flatMap((s) => s.cols), ...(showOther ? other : [])].filter((c) => c.writable && !hasOptions(c) && c !== statusCol).map((c) => c.header);

  const move = (column: string, dir: 1 | -1) => {
    commitEditing();
    const i = fieldOrder.indexOf(column);
    const target = fieldOrder[i + dir];
    if (target === undefined) { focusField(column); return; }
    const v = valueNow(target);
    clearCell(target);
    setBaseline((b) => ({ ...b, [target]: v }));
    setEd({ column: target, base: v, draft: v });
  };

  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (choiceOpen !== null || isTyping(e.target)) return;
      if (e.key === 'Escape') requestClose();
      if ((e.key === 'j' || e.key === 'J') && next !== undefined) { e.preventDefault(); go(next); }
      if ((e.key === 'k' || e.key === 'K') && prev !== undefined) { e.preventDefault(); go(prev); }
    };
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('keydown', onKey); };
  });

  const choose = (c: ColumnConfig, v: string) => {
    const cur = valueNow(c.header);
    setChoiceOpen(null);
    focusField(c.header);
    if (v === cur) return;
    clearCell(c.header);
    void run(c.header, cur, v, true);
  };

  const statusLine = (c: ColumnConfig): ReactNode => {
    const st = cells[c.header];
    const column = c.header;
    if (!st) return null;
    if (st.kind === 'saving') return <p className="text-xs text-pt-muted">保存中…</p>;
    if (st.kind === 'saved') {
      return (
        <p className="flex items-center gap-2 text-xs text-pt-ok">
          <span className="inline-flex items-center gap-1"><IconCheck size={12} />保存しました</span>
          {st.undo && (
            <button type="button" onClick={() => { const u = st.undo; if (u) { clearCell(column); void run(column, u.cur, u.prev); } }} className="inline-flex h-[44px] items-center text-pt-primary hover:underline sm:h-6">元に戻す</button>
          )}
        </p>
      );
    }
    const linkBtn = 'inline-flex h-[44px] items-center rounded px-1 text-[12px] font-medium sm:h-7';
    if (st.kind === 'failed') {
      return (
        <div className="mt-1 flex flex-wrap items-center gap-x-3 border-l-2 border-pt-danger pl-2 text-[12px]">
          <span className="text-pt-danger">保存できませんでした</span>
          <button type="button" className={cx(linkBtn, 'text-pt-primary hover:underline')} onClick={() => { const e = editingRef.current; const v = e?.column === column ? e.draft : st.mine; setEd(null); void run(column, valueNow(column), v); }}>もう一度保存</button>
          <button type="button" className={cx(linkBtn, 'text-pt-muted hover:underline')} onClick={() => { setEd(null); clearCell(column); focusField(column); }}>やめる</button>
        </div>
      );
    }
    return (
      <div className="mt-1 border-l-2 border-pt-warn pl-2 text-[12px]">
        <p className="break-words text-pt-ink">
          <span className="text-pt-warn">シートの値: </span>{shown(st.latest)}
          <span className="text-pt-muted">（{st.by}・<span className="pt-num">{formatShortDateTime(st.at)}</span>）</span>
          <span className="ml-2 text-pt-muted">あなたの値: {shown(st.mine)}</span>
        </p>
        <div className="flex flex-wrap items-center gap-x-3">
          <button type="button" className={cx(linkBtn, 'text-pt-primary hover:underline')} onClick={() => { clearCell(column); setBaseline((b) => ({ ...b, [column]: valueNow(column) })); }}>シートの値にする</button>
          <button type="button" className={cx(linkBtn, 'text-pt-primary hover:underline')} onClick={() => { clearCell(column); void run(column, st.latest, st.mine); }}>自分の値で上書き</button>
        </div>
      </div>
    );
  };

  const updatedMark = (c: ColumnConfig): ReactNode => {
    const st = cells[c.header];
    const updated = (baseline[c.header] ?? '') !== (values[c.header] ?? '') && st?.kind !== 'saving' && st?.kind !== 'conflict';
    return updated ? <span className="mt-1.5 shrink-0 rounded bg-pt-changed px-1 text-[11px] leading-4 whitespace-nowrap text-pt-ink sm:mt-2">更新</span> : null;
  };

  const renderRow = (c: ColumnConfig) => {
    const value = values[c.header] ?? '';
    const st = cells[c.header];
    const isEditing = editing?.column === c.header;
    const multi = c.multiline;
    let body: ReactNode;
    if (c.writable && isEditing && !hasOptions(c)) {
      const onWrapKey = (e: KeyboardEvent) => {
        if (e.key === 'Escape') e.stopPropagation();
        if (e.key === 'Tab' && kindOf(c) !== 'datetime') { e.preventDefault(); move(c.header, e.shiftKey ? -1 : 1); }
      };
      body = (
        <div onKeyDown={onWrapKey} className="[&_textarea]:min-h-[88px] sm:[&_textarea]:h-auto [&_textarea]:[field-sizing:content]">
          <CellInput
            column={c}
            value={editing.draft}
            onChange={(v) => { setEd({ ...editing, draft: v }); }}
            onCommit={() => { move(c.header, 1); }}
            onCancel={() => { setEd(null); focusField(c.header); }}
          />
          {multi && <p className="mt-1 text-[11px] text-pt-faint">Ctrl+Enter で保存・Esc で取り消し</p>}
        </div>
      );
    } else if (c.writable && hasOptions(c)) {
      body = (
        <div className="relative">
          <button
            type="button"
            data-field={c.header}
            disabled={st?.kind === 'saving'}
            onClick={() => { commitEditing(); setChoiceOpen(choiceOpen === c.header ? null : c.header); }}
            className="-ml-2 flex min-h-[44px] w-[calc(100%+0.5rem)] items-center gap-2 rounded px-2 text-left text-[13px] hover:bg-pt-sunken focus-visible:outline-2 focus-visible:outline-pt-select sm:min-h-8"
          >
            <span className="min-w-0 flex-1">{value === '' ? <span className="text-pt-faint">空</span> : <CellDisplay column={c} value={value} />}</span>
            <span className="hidden text-pt-faint opacity-0 group-hover:opacity-100 sm:inline"><IconChevronDown size={13} /></span>
          </button>
          {choiceOpen === c.header && <ChoicePopover options={c.options ?? []} value={value} onSelect={(v) => { choose(c, v); }} onClose={() => { setChoiceOpen(null); focusField(c.header); }} />}
        </div>
      );
    } else if (c.writable) {
      body = (
        <button
          type="button"
          data-field={c.header}
          disabled={st?.kind === 'saving'}
          onClick={() => { startEdit(c.header); }}
          className={cx('-ml-2 flex min-h-[44px] w-[calc(100%+0.5rem)] items-start gap-2 rounded px-2 py-2.5 text-left text-[13px] hover:bg-pt-sunken focus-visible:outline-2 focus-visible:outline-pt-select sm:min-h-8 sm:py-1.5', !multi && 'items-center', st?.kind === 'saved' && 'bg-pt-ok-soft')}
        >
          <span className={cx('min-w-0 flex-1', multi && 'whitespace-pre-wrap break-words leading-relaxed')}>{value === '' ? <span className="text-pt-faint">空</span> : <CellDisplay column={c} value={value} />}</span>
          <span className="hidden shrink-0 text-pt-faint opacity-0 group-hover:opacity-100 sm:inline"><IconPencil size={13} /></span>
        </button>
      );
    } else {
      body = (
        <div className={cx('flex min-h-[44px] items-center text-[13px] text-pt-muted sm:min-h-8', multi && 'items-start py-2.5 whitespace-pre-wrap break-words leading-relaxed sm:py-1.5')}>
          <CellDisplay column={c} value={value} />
        </div>
      );
    }
    return (
      <div key={c.header} className="group grid grid-cols-1 sm:grid-cols-[8rem_1fr] sm:gap-x-3">
        <div className="flex min-h-6 items-center gap-1 pt-1 text-[12px] text-pt-muted sm:min-h-8 sm:pt-0 sm:text-[13px]">
          <span className="min-w-0 break-words">{labelOf(c)}</span>
          {!c.writable && <span className="shrink-0 text-pt-faint" title="読むだけの項目です"><IconLock size={11} /></span>}
        </div>
        <div ref={isEditing ? editWrapRef : undefined} className="min-w-0">
          <div className="flex items-start gap-2">
            <div className="min-w-0 flex-1">{body}</div>
            {updatedMark(c)}
          </div>
          {statusLine(c)}
        </div>
      </div>
    );
  };

  const history = cellHistory(data, sheet.id, rowNumber).filter((e) => e.kind !== 'view');
  const contactBtn = 'inline-flex h-[44px] flex-1 items-center justify-center gap-1.5 rounded-md border border-pt-line-strong bg-pt-surface px-3.5 text-[13px] font-medium text-pt-ink shadow-pt-card hover:bg-pt-bg sm:h-9 sm:flex-none whitespace-nowrap';
  const statusWritable = statusCol !== undefined && statusCol.writable && hasOptions(statusCol);
  const dotClass = statusDot(sheet, statusValue);

  return (
    <div className="fixed inset-0 z-[50] flex justify-end" role="dialog" aria-modal="true" aria-label="応募者の詳細">
      <button type="button" aria-label="閉じる" tabIndex={-1} onClick={requestClose} className="hidden flex-1 cursor-default bg-black/10 sm:block" />
      <div ref={panelRef} className="flex h-full w-full flex-col bg-pt-surface sm:w-[560px] sm:border-l sm:border-pt-line sm:shadow-pt-pop">
        <div className="flex items-center gap-0.5 border-b border-pt-line px-2 py-1">
          <NavButton label="前の応募者（K）" disabled={prev === undefined} onClick={() => { go(prev); }}><span className="rotate-180"><IconChevronDown size={16} /></span></NavButton>
          <NavButton label="次の応募者（J）" disabled={next === undefined} onClick={() => { go(next); }}><IconChevronDown size={16} /></NavButton>
          <span className="pt-num ml-1 shrink-0 text-xs text-pt-faint" title="J で次、K で前の応募者">{index >= 0 ? `${String(index + 1)} / ${String(sequence.length)}` : ''}</span>
          <span className={cx('ml-2 min-w-0 flex-1 truncate text-[13px] font-medium text-pt-ink', scrolled ? 'opacity-100' : 'opacity-0')} aria-hidden={!scrolled}>{name}</span>
          <NavButton label="閉じる（Esc）" onClick={requestClose}><IconX size={16} /></NavButton>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto" onScroll={(e) => { setScrolled(e.currentTarget.scrollTop > 48); }}>
          <div className="border-b border-pt-line px-5 pt-4 pb-4">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-[17px] leading-tight font-semibold text-pt-ink">{name}</h2>
              {live.changedSinceLastView === 'new' && <Badge tone="new">新着</Badge>}
            </div>
            {statusCol && (
              <div className="mt-2">
                <div className="relative inline-flex max-w-full items-center gap-2">
                  {statusWritable ? (
                    <button type="button" data-field={statusCol.header} title="選考状況を変える" onClick={() => { commitEditing(); setChoiceOpen(choiceOpen === statusCol.header ? null : statusCol.header); }} className="inline-flex min-h-[44px] items-center rounded-full focus-visible:outline-2 focus-visible:outline-pt-select sm:min-h-0">
                      <StatusPill value={statusValue} dot={dotClass} />
                    </button>
                  ) : (
                    <StatusPill value={statusValue} dot={dotClass} />
                  )}
                  {updatedMark(statusCol)}
                  {choiceOpen === statusCol.header && <ChoicePopover options={statusCol.options ?? []} value={statusValue} dotFor={(o) => statusDot(sheet, o)} onSelect={(v) => { choose(statusCol, v); }} onClose={() => { setChoiceOpen(null); focusField(statusCol.header); }} />}
                </div>
                {statusLine(statusCol)}
              </div>
            )}
            {meta.length > 0 && <p className="mt-2 text-[13px] text-pt-muted">{meta.join('・')}</p>}
            {(phone !== '' || mail !== '') && (
              <div className="mt-3 flex flex-wrap gap-2">
                {phone !== '' && <a href={`tel:${phone.replace(/[^\d+]/g, '')}`} className={cx(contactBtn, 'basis-full sm:basis-auto')}><IconPhone size={15} />電話する<span className="pt-num font-normal text-pt-muted">{phone}</span></a>}
                {mail !== '' && <a href={`mailto:${mail}`} className={contactBtn}><IconMail size={15} />メールする</a>}
              </div>
            )}
          </div>

          <div className="flex flex-col gap-4 px-5 py-4">
            {sections.map((s) => (
              <section key={s.title}>
                <h3 className="mb-1 text-[12px] font-semibold tracking-wide text-pt-faint">{s.title}</h3>
                <div className="flex flex-col">{s.cols.map(renderRow)}</div>
              </section>
            ))}
            {other.length > 0 && (
              <section>
                <button type="button" onClick={() => { setShowOther((v) => !v); }} className="inline-flex h-[44px] items-center gap-1 text-[12px] font-semibold tracking-wide text-pt-faint hover:text-pt-muted sm:h-8">
                  <span className={cx(showOther && 'rotate-0', !showOther && '-rotate-90')}><IconChevronDown size={14} /></span>そのほか
                </button>
                {showOther && <div className="flex flex-col">{other.map(renderRow)}</div>}
              </section>
            )}
            <section>
              <h3 className="mb-2 text-[12px] font-semibold tracking-wide text-pt-faint">この応募者の記録</h3>
              <Timeline entries={history} labelFor={(h) => { const c = sheet.columns.find((x) => x.header === h); return c ? labelOf(c) : h; }} />
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}

export function RowEditor({ sheet, row, onClose, onMove }: RowEditorProps) {
  const [nav, setNav] = useState<{ from: number; to: number } | null>(null);
  const order = useVisibleRows(sheet.id);
  const current = nav?.from === row.rowNumber ? nav.to : row.rowNumber;
  return (
    <RowDetail
      key={`${sheet.id}-${String(current)}`}
      sheet={sheet}
      rowNumber={current}
      fallback={row}
      order={order}
      onClose={onClose}
      onNavigate={(n) => { if (onMove) onMove(n); else setNav({ from: row.rowNumber, to: n }); }}
    />
  );
}
