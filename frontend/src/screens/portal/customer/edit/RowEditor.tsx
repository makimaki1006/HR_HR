// 応募者 1 人の詳細。項目を「項目名｜値」で並べ、書ける項目は値を押すとその場で直せる。担当: detail チーム。
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { actions, cellHistory, getData, log, stageOf, useData, viewerName } from '../../data';
import { TODAY_NUM, cardLayout, dominantDateStyle, formatDateTimeJa, formatDayJa, groupOf, kindOf, labelOf, parseLooseDay, parseTimeOfDay, type DateStyle } from '../../columns';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { Badge, BottomSheet, EditableBox, cx, formatShortDateTime } from '../../ui/kit';
import { CellDisplay, CellInput } from '../../ui/cell';
import { IconCalendar, IconCheck, IconChevronDown, IconLock, IconMail, IconPhone, IconX } from '../../ui/icons';
import { interviewAt, rowRefDay, statusDot } from '../list/model';
import { StatusSheetList } from '../list/StatusSheet';
import { SmallButton } from '../list/parts';
import { ChoicePopover } from './ChoicePopover';
import { Timeline } from './Timeline';
import { useVisibleRows } from './visibleRows';
import { NOW } from '../../fixtures';

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

const shown = (v: string): string => (v === '' ? '未入力' : v);

/** 画面の幅がスマホか（640px 未満）。 */
function useIsMobile(): boolean {
  const q = '(max-width: 639px)';
  const [m, setM] = useState(() => window.matchMedia(q).matches);
  useEffect(() => {
    const mq = window.matchMedia(q);
    const h = () => { setM(mq.matches); };
    mq.addEventListener('change', h);
    return () => { mq.removeEventListener('change', h); };
  }, []);
  return m;
}

/** 日番号と分から、予定を 1 件持つ .ics の文字（日本時間 → UTC）。 */
function icsText(title: string, dayN: number, minutes: number): string {
  const stamp = (ms: number): string => new Date(ms).toISOString().replace(/[-:]/g, '').replace(/\.\d{3}/, '');
  const start = dayN * 86400000 + minutes * 60000 - 9 * 3600000;
  return ['BEGIN:VCALENDAR', 'VERSION:2.0', 'PRODID:-//applicant-portal//JA', 'BEGIN:VEVENT', `UID:${String(start)}-${encodeURIComponent(title)}@portal`, `DTSTAMP:${stamp(Date.now())}`, `DTSTART:${stamp(start)}`, `DTEND:${stamp(start + 3600000)}`, `SUMMARY:${title}`, 'END:VEVENT', 'END:VCALENDAR'].join('\r\n');
}

function downloadIcs(title: string, dayN: number, minutes: number): void {
  const url = URL.createObjectURL(new Blob([icsText(title, dayN, minutes)], { type: 'text/calendar;charset=utf-8' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = 'interview.ics';
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => { URL.revokeObjectURL(url); }, 1000);
}

/** 長い文を、スマホでは 3 行で切って「続きを読む」で開く。 */
function ClampText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = text.length > 60 || text.split('\n').length > 3;
  return (
    <div>
      <p className={cx('whitespace-pre-wrap break-words leading-relaxed', long && !open && 'line-clamp-3 sm:line-clamp-none')}>{text}</p>
      {long && !open && <button type="button" onClick={() => { setOpen(true); }} className="inline-flex h-[44px] items-center text-[13px] text-pt-primary hover:underline sm:hidden">続きを読む</button>}
    </div>
  );
}

/** data-field を持つ要素（ボタン本体、または箱の外側）の押せるところに焦点を当てる。 */
function focusEl(el: HTMLElement | null | undefined): void {
  if (!el) return;
  if (el.tagName === 'BUTTON') el.focus();
  else el.querySelector<HTMLElement>('button')?.focus();
}

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

function StatusPill({ value, dot, pressable }: { value: string; dot: string; pressable?: boolean }) {
  return (
    <span className="inline-flex h-[28px] max-w-full items-center gap-1.5 rounded-full border border-pt-line bg-pt-surface pr-2 pl-2.5 text-[13px] text-pt-ink sm:h-[24px] sm:text-[12px]">
      {value !== '' && <span className={cx('h-2 w-2 shrink-0 rounded-full', dot)} />}
      <span className={cx('truncate', value === '' && 'text-pt-faint')}>{value === '' ? '状況を選ぶ' : value}</span>
      {pressable && <span className="shrink-0 text-pt-faint"><IconChevronDown size={13} /></span>}
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
  const [summaryOpen, setSummaryOpen] = useState(false);
  const [memo, setMemo] = useState('');
  const [memoFocus, setMemoFocus] = useState(false);
  const isMobile = useIsMobile();
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
    if (window.matchMedia('(min-width: 640px)').matches) focusEl(panelRef.current?.querySelector<HTMLElement>('[data-field]'));
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
    setTimeout(() => { focusEl(panelRef.current?.querySelector<HTMLElement>(`[data-field="${CSS.escape(column)}"]`)); }, 0);
  };

  const run = async (column: string, base: string, value: string, withUndo = false): Promise<boolean> => {
    setCells((c) => ({ ...c, [column]: { kind: 'saving' } }));
    const res = await actions.saveCell(sheet.id, rowNumber, column, base, value);
    const done = (v: string) => {
      setBaseline((b) => ({ ...b, [column]: v }));
      setCells((c) => ({ ...c, [column]: withUndo ? { kind: 'saved', undo: { prev: base, cur: v } } : { kind: 'saved' } }));
      setTimeout(() => { setCells((c) => (c[column]?.kind === 'saved' ? Object.fromEntries(Object.entries(c).filter(([k]) => k !== column)) : c)); }, withUndo ? 5000 : 3000);
    };
    if (res.kind === 'saved') { done(res.value); return true; }
    if (res.kind === 'conflict') {
      if (res.latest === value) { done(value); return true; }
      setCells((c) => ({ ...c, [column]: { kind: 'conflict', latest: res.latest, by: res.by, at: res.at, mine: value } }));
      return false;
    }
    setCells((c) => ({ ...c, [column]: { kind: 'failed', mine: value } }));
    const col = sheet.columns.find((x) => x.header === column);
    if (col && !hasOptions(col)) setEd({ column, base, draft: value });
    return false;
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

  const closeSheet = useCallback(() => { setChoiceOpen(null); }, []);
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
  const refDay = rowRefDay(sheet, live);
  const dateStyles = useMemo(() => {
    const m: Record<string, DateStyle> = {};
    for (const c of sheet.columns) {
      const k = kindOf(c);
      if (k === 'date' || k === 'datetime') m[c.header] = dominantDateStyle(rows, c.header);
    }
    return m;
  }, [sheet.columns, rows]);
  const interviewCol = sheet.interviewColumn ? sheet.columns.find((c) => c.header === sheet.interviewColumn) : undefined;
  const interviewRaw = interviewCol ? (values[interviewCol.header] ?? '') : '';
  const interviewTimeRaw = sheet.interviewTimeColumn ? (values[sheet.interviewTimeColumn] ?? '') : undefined;
  const interviewF = interviewRaw !== '' ? formatDateTimeJa(interviewRaw, interviewTimeRaw, refDay) : null;
  const interviewDay = interviewRaw !== '' ? parseLooseDay(interviewRaw, refDay) : null;
  const interviewMin = interviewRaw !== '' ? parseTimeOfDay(interviewTimeRaw ?? interviewRaw.replace(/^(\d{4}\s*[年/.-]\s*)?\d{1,2}\s*[月/.-]\s*\d{1,2}\s*日?/, '')) : null;
  const interviewPast = interviewDay !== null && interviewDay < TODAY_NUM;
  const interviewMs = interviewAt(sheet, live);
  const stageNow = statusHeader ? stageOf(sheet, statusValue) : undefined;
  /** カレンダーに足せるのは、これからの面接で、辞退・不採用になっていないときだけ。 */
  const canAddCalendar = interviewMs !== null && interviewMs > Date.parse(NOW) && stageNow !== 'rejected' && stageNow !== 'declined';
  const noteCol = sheet.columns.find((c) => c.writable && c.header.includes('備考'));
  const metaCols = visible.filter((c) => groupOf(c) === 'summary' && c !== titleCol && c !== phoneCol && c !== mailCol && c !== statusCol);
  const meta = metaCols.map((c) => {
    const v = values[c.header] ?? '';
    if (v === '') return '';
    const k = kindOf(c);
    if (k === 'date' || k === 'datetime') {
      const f = formatDayJa(v, refDay);
      const t = f ? f.text : v;
      return c.header === sheet.sortColumn ? `${t}応募` : t;
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
    if (st.kind === 'saving') return <p className="text-[13px] text-pt-muted">保存中…</p>;
    if (st.kind === 'saved') {
      return (
        <p className="flex items-center gap-2 text-[13px] text-pt-ok">
          <span className="inline-flex items-center gap-1"><IconCheck size={12} />保存しました</span>
          {st.undo && (
            <button type="button" onClick={() => { const u = st.undo; if (u) { clearCell(column); void run(column, u.cur, u.prev); } }} className="inline-flex h-[44px] items-center text-pt-primary hover:underline sm:h-6">元に戻す</button>
          )}
        </p>
      );
    }
    if (st.kind === 'failed') {
      return (
        <div className="mt-1 flex flex-wrap items-center gap-x-3 border-l-2 border-pt-danger pl-2 text-[13px] sm:text-[12px]">
          <span className="text-pt-danger">保存できませんでした</span>
          <div className="grid w-full grid-cols-2 gap-2 py-1 sm:flex sm:w-auto">
            <SmallButton primary onClick={() => { const e = editingRef.current; const v = e?.column === column ? e.draft : st.mine; setEd(null); void run(column, valueNow(column), v); }}>もう一度保存</SmallButton>
            <SmallButton onClick={() => { setEd(null); clearCell(column); focusField(column); }}>やめる</SmallButton>
          </div>
        </div>
      );
    }
    return (
      <div className="mt-1 border-l-2 border-pt-warn pl-2 text-[13px] sm:text-[12px]">
        <p className="break-words text-pt-ink">
          <span className="text-pt-warn">シートの値: </span>{shown(st.latest)}
          <span className="text-pt-muted">（{st.by}・<span className="pt-num">{formatShortDateTime(st.at)}</span>）</span>
          <span className="block text-pt-muted sm:ml-2 sm:inline"><span className="whitespace-nowrap">あなたの値:</span> {shown(st.mine)}</span>
        </p>
        <div className="grid grid-cols-2 gap-2 py-1.5 sm:flex">
          <SmallButton onClick={() => { clearCell(column); setBaseline((b) => ({ ...b, [column]: valueNow(column) })); }}>シートの値にする</SmallButton>
          <SmallButton onClick={() => { clearCell(column); void run(column, st.latest, st.mine); }}>自分の値で上書き</SmallButton>
        </div>
      </div>
    );
  };

  const updatedMark = (c: ColumnConfig): ReactNode => {
    const st = cells[c.header];
    const updated = (baseline[c.header] ?? '') !== (values[c.header] ?? '') && st?.kind !== 'saving' && st?.kind !== 'conflict';
    return updated ? <span className="mt-1.5 shrink-0 rounded bg-pt-changed px-1 text-[12px] leading-4 whitespace-nowrap text-pt-ink sm:mt-2">更新</span> : null;
  };

  const renderRow = (c: ColumnConfig) => {
    const value = values[c.header] ?? '';
    const st = cells[c.header];
    const isEditing = editing?.column === c.header;
    const multi = c.multiline;
    let body: ReactNode;
    const saving = st?.kind === 'saving';
    const savedTone = st?.kind === 'saved' ? 'bg-pt-ok-soft' : undefined;
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
            dateStyle={dateStyles[c.header] ?? 'slash'}
            refDay={refDay}
            onChange={(v) => { setEd({ ...editing, draft: v }); }}
            onCommit={() => { move(c.header, 1); }}
            onCancel={() => { setEd(null); focusField(c.header); }}
          />
          {multi && <p className="mt-1 hidden text-[12px] text-pt-faint sm:block">Ctrl+Enter で保存・Esc で取り消し</p>}
        </div>
      );
    } else if (c.writable && hasOptions(c)) {
      body = (
        <div className="relative" data-field={c.header}>
          <EditableBox
            label={labelOf(c)}
            className={cx(savedTone)}
            onActivate={() => { if (saving) return; commitEditing(); setChoiceOpen(choiceOpen === c.header ? null : c.header); }}
          >
            <CellDisplay column={c} value={value} refDay={refDay} empty="input" className="text-[15px] sm:text-[13px]" />
          </EditableBox>
          {choiceOpen === c.header && !isMobile && <ChoicePopover options={c.options ?? []} value={value} onSelect={(v) => { choose(c, v); }} onClose={() => { setChoiceOpen(null); focusField(c.header); }} />}
        </div>
      );
    } else if (c.writable) {
      body = (
        <div data-field={c.header}>
          <EditableBox label={labelOf(c)} className={cx(multi && 'items-start py-2', savedTone)} onActivate={() => { if (!saving) startEdit(c.header); }}>
            <span className={cx('block text-[15px] sm:text-[13px]', multi && 'whitespace-pre-wrap break-words leading-relaxed')}>
              <CellDisplay column={c} value={value} refDay={refDay} empty="input" />
            </span>
          </EditableBox>
        </div>
      );
    } else {
      body = (
        <div className={cx('flex min-h-[44px] items-center text-[15px] text-pt-muted sm:min-h-8 sm:text-[13px]', multi && 'items-start py-2.5 sm:py-1.5')}>
          {multi && value !== '' ? <ClampText text={value} /> : <CellDisplay column={c} value={value} refDay={refDay} />}
        </div>
      );
    }
    return (
      <div key={c.header} className="group grid grid-cols-1 sm:grid-cols-[8rem_1fr] sm:gap-x-3">
        <div className="flex min-h-6 items-center gap-1 pt-1 text-[13px] text-pt-muted sm:min-h-8 sm:pt-0">
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
  const contactBtn = 'inline-flex h-[44px] min-w-0 flex-1 items-center justify-center gap-1.5 rounded-md border border-pt-line-strong bg-pt-surface px-3.5 text-[15px] font-medium text-pt-ink shadow-pt-card hover:bg-pt-bg sm:h-9 sm:flex-none sm:text-[13px] whitespace-nowrap';
  const statusWritable = statusCol !== undefined && statusCol.writable && hasOptions(statusCol);
  const dotClass = statusDot(sheet, statusValue);
  const sheetCol = choiceOpen !== null ? sheet.columns.find((c) => c.header === choiceOpen) : undefined;
  const sheetIsStatus = sheetCol !== undefined && sheetCol === statusCol;

  const memoText = memo.trim();
  const memoStamp = `${(() => { const d = new Date(TODAY_NUM * 86400000); return `${String(d.getUTCMonth() + 1).padStart(2, '0')}/${String(d.getUTCDate()).padStart(2, '0')}`; })()} 追記（${(viewerName().split('@')[0] ?? '').split(/\s+/).pop() ?? ''}）：`;
  const submitMemo = () => {
    if (!noteCol || memoText === '') return;
    const base = valueNow(noteCol.header);
    const next = `${base}${base === '' ? '' : '\n'}${memoStamp}${memoText}`;
    clearCell(noteCol.header);
    setMemo('');
    setMemoFocus(false);
    void run(noteCol.header, base, next);
  };

  const summaryParts = metaCols.length > 0 ? meta.join('・') : '';

  return (
    <div className="fixed inset-0 z-[50] flex justify-end" role="dialog" aria-modal="true" aria-label="応募者の詳細">
      <button type="button" aria-label="閉じる" tabIndex={-1} onClick={requestClose} className="hidden flex-1 cursor-default bg-black/10 sm:block" />
      <div ref={panelRef} className="flex h-full w-full flex-col bg-pt-surface sm:w-[560px] sm:border-l sm:border-pt-line sm:shadow-pt-pop">
        <div className="flex items-center gap-0.5 border-b border-pt-line px-2 py-1">
          <span className="hidden sm:inline-flex"><NavButton label="前の応募者（K）" disabled={prev === undefined} onClick={() => { go(prev); }}><span className="rotate-180"><IconChevronDown size={16} /></span></NavButton></span>
          <span className="hidden sm:inline-flex"><NavButton label="次の応募者（J）" disabled={next === undefined} onClick={() => { go(next); }}><IconChevronDown size={16} /></NavButton></span>
          <span className="pt-num ml-1 hidden shrink-0 text-[12px] text-pt-faint sm:inline" title="J で次、K で前の応募者">{index >= 0 ? `${String(index + 1)} / ${String(sequence.length)}` : ''}</span>
          <span className={cx('ml-2 min-w-0 flex-1 truncate text-[15px] font-medium text-pt-ink sm:text-[13px]', scrolled ? 'opacity-100' : 'opacity-0')} aria-hidden={!scrolled}>{name}</span>
          <NavButton label="閉じる（Esc）" onClick={requestClose}><IconX size={16} /></NavButton>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto" onScroll={(e) => { setScrolled(e.currentTarget.scrollTop > 48); }}>
          <div className="border-b border-pt-line px-4 pt-4 pb-4 sm:px-5">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-[16px] leading-tight font-semibold text-pt-ink sm:text-[17px]">{name}</h2>
              {live.changedSinceLastView === 'new' && <Badge tone="new">新着</Badge>}
            </div>
            {statusCol && (
              <div className="mt-2">
                <div className="relative inline-flex max-w-full items-center gap-2">
                  {statusWritable ? (
                    <button type="button" data-field={statusCol.header} title="選考状況を変える" aria-label={`選考状況を変える（いまは${statusValue === '' ? '未入力' : statusValue}）`} onClick={() => { commitEditing(); setChoiceOpen(choiceOpen === statusCol.header ? null : statusCol.header); }} className="inline-flex min-h-[44px] items-center rounded-full focus-visible:outline-2 focus-visible:outline-pt-select sm:min-h-0">
                      <StatusPill value={statusValue} dot={dotClass} pressable />
                    </button>
                  ) : (
                    <StatusPill value={statusValue} dot={dotClass} />
                  )}
                  {updatedMark(statusCol)}
                  {choiceOpen === statusCol.header && !isMobile && <ChoicePopover options={statusCol.options ?? []} value={statusValue} dotFor={(o) => statusDot(sheet, o)} onSelect={(v) => { choose(statusCol, v); }} onClose={() => { setChoiceOpen(null); focusField(statusCol.header); }} />}
                </div>
                {statusLine(statusCol)}
              </div>
            )}
            {interviewF && (
              <p className={cx('mt-2 flex flex-wrap items-center gap-x-3 text-[15px] sm:text-[13px]', interviewPast ? 'text-pt-muted' : 'text-pt-ink')}>
                <span><span className="text-pt-muted">面接 </span><span className={cx('pt-num', interviewF.isToday && 'font-semibold')}>{interviewF.text}</span></span>
                {canAddCalendar && interviewDay !== null && interviewMin !== null && (
                  <button type="button" onClick={() => { downloadIcs(`面接 ${name}さん`, interviewDay, interviewMin); }} className="inline-flex h-[44px] items-center gap-1 text-[13px] text-pt-primary hover:underline sm:h-6 sm:text-[12px]">
                    <IconCalendar size={13} />カレンダーに追加
                  </button>
                )}
              </p>
            )}
            {(phone !== '' || mail !== '') && (
              <div className="mt-3 flex gap-2">
                {phone !== '' && <a href={`tel:${phone.replace(/[^\d+]/g, '')}`} className={contactBtn}><IconPhone size={15} />電話する<span className="pt-num hidden font-normal text-pt-muted sm:inline">{phone}</span></a>}
                {mail !== '' && <a href={`mailto:${mail}`} className={contactBtn}><IconMail size={15} />メールする</a>}
              </div>
            )}
          </div>

          <div className="flex flex-col gap-5 px-4 py-4 sm:gap-4 sm:px-5">
            {sections.map((s) => {
              const collapsible = s.title === '応募の内容';
              const closed = collapsible && !summaryOpen;
              return (
                <section key={s.title}>
                  {collapsible ? (
                    <>
                      <button type="button" onClick={() => { setSummaryOpen((v) => !v); }} aria-expanded={summaryOpen} className="flex min-h-[44px] w-full items-center gap-1 text-left sm:hidden">
                        <span className={cx('shrink-0 text-pt-faint', closed && '-rotate-90')}><IconChevronDown size={14} /></span>
                        <span className="shrink-0 text-[13px] font-semibold tracking-wide text-pt-faint">{s.title}</span>
                        {closed && summaryParts !== '' && <span className="ml-2 min-w-0 flex-1 truncate text-[13px] text-pt-muted">{summaryParts}</span>}
                      </button>
                      <h3 className="mb-1 hidden text-[12px] font-semibold tracking-wide text-pt-faint sm:block">{s.title}</h3>
                    </>
                  ) : (
                    <h3 className="mb-1 text-[13px] font-semibold tracking-wide text-pt-faint sm:text-[12px]">{s.title}</h3>
                  )}
                  <div className={cx('flex-col sm:gap-1.5', closed ? 'hidden sm:flex' : 'flex')}>{s.cols.map(renderRow)}</div>
                </section>
              );
            })}
            {other.length > 0 && (
              <section>
                <button type="button" onClick={() => { setShowOther((v) => !v); }} className="inline-flex h-[44px] items-center gap-1 text-[13px] font-semibold tracking-wide text-pt-faint hover:text-pt-muted sm:h-8 sm:text-[12px]">
                  <span className={cx(showOther && 'rotate-0', !showOther && '-rotate-90')}><IconChevronDown size={14} /></span>そのほか
                </button>
                {showOther && <div className="flex flex-col sm:gap-1.5">{other.map(renderRow)}</div>}
              </section>
            )}
            <section>
              <h3 className="mb-2 text-[13px] font-semibold tracking-wide text-pt-faint sm:text-[12px]">この応募者の記録</h3>
              {noteCol && (
                <div className="mb-4">
                  <textarea
                    value={memo}
                    onChange={(e) => { setMemo(e.target.value); }}
                    onFocus={() => { setMemoFocus(true); }}
                    onKeyDown={(e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); submitMemo(); } }}
                    rows={memoFocus || memo !== '' ? 3 : 1}
                    placeholder="メモを追記"
                    aria-label="メモを追記"
                    className="w-full resize-none rounded-[6px] border border-pt-field bg-pt-surface px-[10px] py-[9px] text-base leading-relaxed outline-none placeholder:text-pt-faint hover:border-pt-line-strong focus:border-pt-select focus:ring-2 focus:ring-pt-select/20 sm:py-[6px] sm:text-[13px]"
                  />
                  <div className="mt-1 flex items-start gap-3">
                    <p className="min-w-0 flex-1 text-[13px] leading-snug text-pt-faint sm:text-[12px]">シートの備考の末尾に「{memoStamp}…」の形で足します</p>
                    <button type="button" onClick={submitMemo} disabled={memoText === '' || cells[noteCol.header]?.kind === 'saving'} className="inline-flex h-[44px] shrink-0 items-center rounded-md bg-pt-primary px-3.5 text-[15px] font-medium text-white shadow-pt-card hover:bg-pt-primary-hover disabled:cursor-not-allowed disabled:opacity-45 sm:h-9 sm:text-[13px]">追記する</button>
                  </div>
                  {statusLine(noteCol)}
                </div>
              )}
              <Timeline entries={history} labelFor={(h) => { const c = sheet.columns.find((x) => x.header === h); return c ? labelOf(c) : h; }} />
            </section>
          </div>
        </div>

        <div className="flex h-[56px] shrink-0 items-center border-t border-pt-line bg-pt-surface px-2 pb-[env(safe-area-inset-bottom)] sm:hidden">
          <button type="button" disabled={prev === undefined} onClick={() => { go(prev); }} aria-label="前の応募者" className="inline-flex h-[44px] min-w-[88px] items-center justify-center gap-1 rounded-md text-[15px] text-pt-ink hover:bg-pt-sunken disabled:opacity-40"><span className="rotate-90"><IconChevronDown size={16} /></span>前</button>
          <span className="pt-num min-w-0 flex-1 text-center text-[15px] text-pt-muted">{index >= 0 ? `${String(index + 1)} / ${String(sequence.length)}` : ''}</span>
          <button type="button" disabled={next === undefined} onClick={() => { go(next); }} aria-label="次の応募者" className="inline-flex h-[44px] min-w-[88px] items-center justify-center gap-1 rounded-md text-[15px] text-pt-ink hover:bg-pt-sunken disabled:opacity-40">次<span className="-rotate-90"><IconChevronDown size={16} /></span></button>
        </div>
      </div>
      <BottomSheet
        open={isMobile && sheetCol !== undefined}
        title={sheetCol ? (sheetIsStatus ? `${name}さんの状況` : labelOf(sheetCol)) : ''}
        onClose={closeSheet}
      >
        {sheetCol && sheetIsStatus && (
          <StatusSheetList sheet={sheet} options={sheetCol.options ?? []} value={values[sheetCol.header] ?? ''} onSelect={(v) => { choose(sheetCol, v); }} />
        )}
        {sheetCol && !sheetIsStatus && (
          <ChoicePopover
            variant="sheet"
            options={sheetCol.options ?? []}
            value={values[sheetCol.header] ?? ''}
            onSelect={(v) => { choose(sheetCol, v); }}
            onClose={closeSheet}
          />
        )}
      </BottomSheet>
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
