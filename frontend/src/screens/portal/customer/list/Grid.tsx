// PC の表。スプレッドシートのように、マスを選んでそのまま書き換えられる。
// マスを押すと選択、矢印・Tab で移動、Enter かダブルクリックで編集、Esc で取り消し、Enter で保存。
// キーボードは表の中の見えない入力欄（catcher）が受ける。日本語入力でも最初の文字から入るように、
// 文字の列はその入力欄がそのままマスの上に出て編集欄になる（Google スプレッドシートと同じ作り）。
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react';
import { kindOf, labelOf } from '../../columns';
import { actions } from '../../data';
import { CellDisplay, CellInput } from '../../ui/cell';
import { IconCheck, IconColumns, IconFilter, IconLock, IconSearch } from '../../ui/icons';
import { cx } from '../../ui/kit';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { IconArrowDown, IconArrowUp, IconExpand } from './icons';
import { columnWidths, statusDot, TITLE_MIN, type ColumnFilter, type SortSpec } from './model';
import { CellMark, ChangeMark, changeLine, SaveProblem, StatusPill, useOutside } from './parts';
import { cellKey, type CellSaver } from './useCellSaver';

interface Pos { rowNumber: number; column: string }
type EditMode = 'text' | 'widget' | 'choice';
interface Editing extends Pos { value: string; base: string; mode: EditMode; query: string }
interface Box { left: number; top: number; width: number; height: number }

/** 値の種類が少ない文字の列は、絞り込みをチェックの一覧にする。 */
const CHECK_LIST_MAX = 20;
const SCROLLED_SHADOW = '4px 0 4px -2px var(--color-pt-line-strong)';

const modeOf = (c: ColumnConfig): EditMode => {
  if (c.options && c.options.length > 0) return 'choice';
  const k = kindOf(c);
  return k === 'date' || k === 'datetime' ? 'widget' : 'text';
};

export interface GridProps {
  sheet: SheetConfig;
  rows: ApplicantRow[];
  /** フィルタの件数に使う、絞り込む前の行。 */
  allRows: ApplicantRow[];
  cols: ColumnConfig[];
  titleHeader: string;
  readOnly: boolean;
  saver: CellSaver;
  sort: SortSpec | null;
  onSort: (s: SortSpec | null) => void;
  filters: Record<string, ColumnFilter>;
  onFilter: (header: string, f: ColumnFilter | null) => void;
  onOpen: (rowNumber: number) => void;
  /** 詳細で開いている行（色を付ける）。 */
  openRowNumber?: number | null;
  /** 見出しのメニューに「この項目の設定」を出すか。 */
  canEditColumns: boolean;
  onColumnSettings: (header: string) => void;
  /** この行へ移って選ぶ（新しい応募者の「見る」）。seq が変わるたびに動く。 */
  jump: { rowNumber: number; seq: number } | null;
  height: number;
}

export function Grid({ sheet, rows, allRows, cols, titleHeader, readOnly, saver, sort, onSort, filters, onFilter, onOpen, openRowNumber = null, canEditColumns, onColumnSettings, jump, height }: GridProps) {
  const [sel, setSel] = useState<Pos | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [menu, setMenu] = useState<string | null>(null);
  const [scrolled, setScrolled] = useState(false);
  const [idleText, setIdleText] = useState('');
  const [catcherBox, setCatcherBox] = useState<Box>({ left: 0, top: 0, width: 0, height: 0 });
  const editingRef = useRef<Editing | null>(null);
  const composingRef = useRef(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const catcherRef = useRef<HTMLTextAreaElement>(null);
  const widths = useMemo(() => columnWidths(cols, allRows, titleHeader, sheet.statusColumn ?? undefined), [cols, allRows, titleHeader, sheet.statusColumn]);
  const widthOf = (c: ColumnConfig): number => widths[c.header] ?? 128;
  const titleW = widths[titleHeader] ?? TITLE_MIN;
  const totalW = cols.reduce((s, c) => s + widthOf(c), 0);

  const rowIndex = useMemo(() => new Map(rows.map((r, i) => [r.rowNumber, i])), [rows]);
  const colIndex = useMemo(() => new Map(cols.map((c, i) => [c.header, i])), [cols]);

  const nameOf = (rowNumber: number): string => rows.find((r) => r.rowNumber === rowNumber)?.values[titleHeader] ?? '';
  const labelFor = (rowNumber: number, column: string): string => {
    const c = cols.find((x) => x.header === column);
    return `${nameOf(rowNumber)}さんの「${c ? labelOf(c) : column}」`;
  };

  const valueOf = (row: ApplicantRow, column: string): string => {
    const s = saver.cells[cellKey(row.rowNumber, column)];
    if (s?.kind === 'saving') return s.value;
    return row.values[column] ?? '';
  };

  const cellEl = (p: Pos | null): HTMLElement | null => {
    if (!p) return null;
    const r = rowIndex.get(p.rowNumber);
    const c = colIndex.get(p.column);
    if (r === undefined || c === undefined) return null;
    return boxRef.current?.querySelector<HTMLElement>(`[data-cell="${String(r)}:${String(c)}"]`) ?? null;
  };

  // シートで変わったマスを選んだら、変わった印（色）を消す。
  useEffect(() => {
    if (sel) actions.markCellSeen(sheet.id, sel.rowNumber, sel.column);
  }, [sel, sheet.id]);

  // 選んだマスを見える位置に。
  useEffect(() => {
    cellEl(sel)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel, rowIndex, colIndex]);

  // 見えない入力欄を、選んだマスの上に置く（日本語入力の候補もそこに出る）。
  const textEditing = editing?.mode === 'text' ? editing : null;
  useLayoutEffect(() => {
    const box = boxRef.current;
    const el = cellEl(sel);
    if (!box || !el) return;
    const b = box.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    const col = cols.find((c) => c.header === sel?.column);
    const tall = textEditing !== null && col?.multiline === true;
    const width = tall ? Math.max(r.width, 300) : r.width;
    let left = r.left - b.left + box.scrollLeft;
    if (left + width > box.scrollLeft + box.clientWidth - 4) left = Math.max(box.scrollLeft, r.right - b.left + box.scrollLeft - width);
    setCatcherBox({ left, top: r.top - b.top + box.scrollTop, width, height: tall ? 112 : r.height });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel, textEditing?.rowNumber, textEditing?.column, rows, cols, scrolled]);

  useEffect(() => {
    if (!textEditing) return;
    const t = catcherRef.current;
    if (!t) return;
    t.focus({ preventScroll: true });
    if (!composingRef.current) t.setSelectionRange(t.value.length, t.value.length);
  }, [textEditing?.rowNumber, textEditing?.column]); // eslint-disable-line react-hooks/exhaustive-deps

  const focusGrid = () => { catcherRef.current?.focus({ preventScroll: true }); };

  // 新しい応募者の「見る」: 一番上へ移り、その行を選ぶ。
  const [seenJump, setSeenJump] = useState(jump?.seq ?? 0);
  if (jump && jump.seq !== seenJump) {
    setSeenJump(jump.seq);
    setSel({ rowNumber: jump.rowNumber, column: titleHeader });
  }
  useEffect(() => {
    if (!jump) return;
    boxRef.current?.scrollTo({ top: 0, left: 0 });
    focusGrid();
  }, [jump?.seq]); // eslint-disable-line react-hooks/exhaustive-deps

  // 吹き出しの外を押したら閉じる（マスの右上の印は残る。もう一度押すと開く）。
  const bubble = saver.bubble;
  const setBubble = saver.setBubble;
  useEffect(() => {
    if (bubble === null) return undefined;
    const h = (e: MouseEvent) => {
      const own = boxRef.current?.querySelector(`[data-key="${CSS.escape(bubble)}"]`);
      if (own && e.target instanceof Node && own.contains(e.target)) return;
      setBubble(null);
    };
    document.addEventListener('mousedown', h, true);
    return () => { document.removeEventListener('mousedown', h, true); };
  }, [bubble, setBubble]);

  /** 吹き出し・入力欄が表の端からはみ出すなら、向きを変える。 */
  const fitInBox = (side: 'right' | 'below') => (el: HTMLElement | null) => {
    const box = boxRef.current;
    if (!el || !box) return;
    const b = box.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    if (side === 'right' && r.right > b.right - 8) { el.style.left = 'auto'; el.style.right = '100%'; el.style.marginLeft = '0'; el.style.marginRight = '4px'; }
    if (side === 'below' && r.right > b.right - 8) { el.style.left = 'auto'; el.style.right = '0'; }
    if (r.bottom > b.bottom - 8 && r.top - b.top > r.height + 40) { el.style.top = 'auto'; el.style.bottom = '100%'; el.style.marginTop = '0'; el.style.marginBottom = '4px'; }
  };

  const move = (dr: number, dc: number, wrap = false) => {
    if (rows.length === 0 || cols.length === 0) return;
    let r = sel ? (rowIndex.get(sel.rowNumber) ?? 0) : 0;
    let c = sel ? (colIndex.get(sel.column) ?? 0) : 0;
    if (sel) {
      r += dr;
      c += dc;
      if (wrap && c >= cols.length) { c = 0; r += 1; }
      if (wrap && c < 0) { c = cols.length - 1; r -= 1; }
    }
    r = Math.max(0, Math.min(rows.length - 1, r));
    c = Math.max(0, Math.min(cols.length - 1, c));
    const row = rows[r];
    const col = cols[c];
    if (row && col) setSel({ rowNumber: row.rowNumber, column: col.header });
    saver.setBubble(null);
  };

  /** 書けないマスなら理由を知らせて false。 */
  const canWrite = (col: ColumnConfig): boolean => {
    if (readOnly) { saver.notify('いまは書き換えられません。シートを読み直してからお試しください'); return false; }
    if (!col.writable) { saver.notify(`「${labelOf(col)}」は弊社がシートで記入する項目です`); return false; }
    return true;
  };

  const startEdit = (pos: Pos, opts: { initial?: string; query?: string } = {}) => {
    const col = cols.find((c) => c.header === pos.column);
    const row = rows.find((r) => r.rowNumber === pos.rowNumber);
    if (!col || !row) return;
    if (col.header === titleHeader) { onOpen(row.rowNumber); return; }
    if (!canWrite(col)) return;
    const base = row.values[col.header] ?? '';
    const e: Editing = { ...pos, base, value: opts.initial ?? base, mode: modeOf(col), query: opts.query ?? '' };
    editingRef.current = e;
    setEditing(e);
    setIdleText('');
    saver.setBubble(null);
  };

  const setEditValue = (v: string) => {
    const e = editingRef.current;
    if (!e) return;
    const next = { ...e, value: v };
    editingRef.current = next;
    setEditing(next);
  };

  const commit = (value: string, then: 'down' | 'right' | 'left' | null) => {
    const e = editingRef.current;
    if (!e) return;
    editingRef.current = null;
    setEditing(null);
    setIdleText('');
    saver.save(e.rowNumber, e.column, e.base, value, labelFor(e.rowNumber, e.column));
    focusGrid();
    if (then === 'down') move(1, 0);
    if (then === 'right') move(0, 1, true);
    if (then === 'left') move(0, -1, true);
  };

  const cancel = () => {
    editingRef.current = null;
    setEditing(null);
    setIdleText('');
    focusGrid();
  };

  /** 選んだマスで文字が打たれた（日本語の確定後も含む）。 */
  const typed = (text: string) => {
    if (!sel || text === '') return;
    const col = cols.find((c) => c.header === sel.column);
    if (!col || col.header === titleHeader) { setIdleText(''); return; }
    if (!canWrite(col)) { setIdleText(''); return; }
    const mode = modeOf(col);
    if (mode === 'choice') { startEdit(sel, { query: text }); return; }
    if (mode === 'widget') { startEdit(sel); return; }
    startEdit(sel, { initial: text });
  };

  const onKey = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing || e.key === 'Process') return;
    const k = e.key;
    const mod = e.ctrlKey || e.metaKey;
    const ed = editingRef.current;
    if (ed?.mode === 'text') {
      const col = cols.find((c) => c.header === ed.column);
      if (k === 'Escape') { e.preventDefault(); cancel(); return; }
      if (k === 'Enter' && (!col?.multiline || mod)) { e.preventDefault(); commit(ed.value, 'down'); return; }
      if (k === 'Tab') { e.preventDefault(); commit(ed.value, e.shiftKey ? 'left' : 'right'); }
      return;
    }
    if (ed) return;
    if (k === 'ArrowDown') { e.preventDefault(); move(1, 0); return; }
    if (k === 'ArrowUp') { e.preventDefault(); move(-1, 0); return; }
    if (k === 'ArrowRight') { e.preventDefault(); move(0, 1); return; }
    if (k === 'ArrowLeft') { e.preventDefault(); move(0, -1); return; }
    if (k === 'Tab') { e.preventDefault(); move(0, e.shiftKey ? -1 : 1, true); return; }
    if (k === 'Escape') { saver.setBubble(null); setMenu(null); return; }
    if (mod && k.toLowerCase() === 'z' && !e.shiftKey) { e.preventDefault(); saver.undo(); return; }
    if (!sel) return;
    const col = cols.find((c) => c.header === sel.column);
    if (!col) return;
    if (k === 'Enter' || k === 'F2') { e.preventDefault(); startEdit(sel); return; }
    if (mod && k.toLowerCase() === 'c') {
      e.preventDefault();
      const row = rows.find((r) => r.rowNumber === sel.rowNumber);
      if (row) void navigator.clipboard.writeText(row.values[sel.column] ?? '').catch(() => undefined);
      return;
    }
    if (mod) return;
    if (k === 'Delete' || k === 'Backspace') {
      e.preventDefault();
      if (col.header === titleHeader || !canWrite(col)) return;
      const row = rows.find((r) => r.rowNumber === sel.rowNumber);
      if (row) saver.save(row.rowNumber, col.header, row.values[col.header] ?? '', '', labelFor(row.rowNumber, col.header));
      return;
    }
    if (k === ' ' && modeOf(col) !== 'text') { e.preventDefault(); startEdit(sel); }
    // ほかの文字は入力欄に入れ、onChange で編集を始める。
  };

  const catcherStyle: CSSProperties = { left: catcherBox.left, top: catcherBox.top, width: catcherBox.width, height: catcherBox.height };
  const editingCol = textEditing ? cols.find((c) => c.header === textEditing.column) : undefined;

  return (
    <div
      ref={boxRef}
      onScroll={(e) => { const s = e.currentTarget.scrollLeft > 0; if (s !== scrolled) setScrolled(s); }}
      className="relative overflow-auto bg-pt-surface"
      style={{ height, scrollPaddingTop: 37, scrollPaddingLeft: titleW }}
    >
      <table className="pt-grid table-fixed border-separate border-spacing-0 text-[13px] leading-5" style={{ width: totalW }}>
        <colgroup>
          {cols.map((c) => <col key={c.header} style={{ width: widthOf(c) }} />)}
        </colgroup>
        <thead>
          <tr>
            {cols.map((c) => (
              <HeaderCell
                key={c.header}
                col={c}
                sticky={c.header === titleHeader}
                scrolled={scrolled}
                sort={sort?.column === c.header ? sort.dir : null}
                filter={filters[c.header]}
                menuOpen={menu === c.header}
                onMenu={(open) => { setMenu(open ? c.header : null); }}
                onSort={(dir) => { onSort(dir ? { column: c.header, dir } : null); }}
                onFilter={(f) => { onFilter(c.header, f); }}
                onSettings={canEditColumns ? () => { setMenu(null); onColumnSettings(c.header); } : null}
                allRows={allRows}
              />
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, ri) => {
            const rowSelected = sel?.rowNumber === row.rowNumber;
            const rowOpen = openRowNumber === row.rowNumber;
            const line = changeLine(row.changedSinceLastView);
            return (
              <tr key={row.rowNumber} className="group">
                {cols.map((c, ci) => {
                  const key = cellKey(row.rowNumber, c.header);
                  const st = saver.cells[key];
                  const isTitle = c.header === titleHeader;
                  const isSel = rowSelected && sel.column === c.header;
                  const isEdit = editing?.rowNumber === row.rowNumber && editing.column === c.header;
                  const v = valueOf(row, c.header);
                  const changedCell = row.changedColumns?.includes(c.header) === true;
                  const bg = rowSelected || rowOpen ? 'bg-pt-primary-soft' : changedCell ? 'bg-pt-changed' : isTitle || c.writable ? 'bg-pt-surface' : 'bg-pt-sunken/35';
                  const shadows = isTitle ? [line ? `inset 2px 0 0 ${line}` : '', scrolled ? SCROLLED_SHADOW : ''].filter((x) => x !== '').join(', ') : '';
                  return (
                    <td
                      key={c.header}
                      data-cell={`${String(ri)}:${String(ci)}`}
                      data-key={key}
                      title={changedCell ? 'シートで変わりました' : undefined}
                      onMouseDown={(e) => {
                        if (isEdit) return;
                        if (editingRef.current) commit(editingRef.current.value, null);
                        if (e.target instanceof Element && e.target.closest('[data-bubble]')) return;
                        e.preventDefault();
                        setSel({ rowNumber: row.rowNumber, column: c.header });
                        focusGrid();
                        if (st?.kind === 'conflict' || st?.kind === 'failed') saver.setBubble(key);
                      }}
                      onDoubleClick={() => { if (!isTitle) startEdit({ rowNumber: row.rowNumber, column: c.header }); }}
                      className={cx(
                        'h-[36px] border-b border-r border-pt-line px-2.5 align-middle',
                        isTitle ? 'sticky left-0 z-10' : 'relative',
                        isTitle && scrolled && 'border-r-pt-line-strong',
                        bg,
                        c.writable && !readOnly ? 'cursor-cell' : 'cursor-default',
                      )}
                      style={shadows !== '' ? { boxShadow: shadows } : undefined}
                    >
                      <div className="flex min-w-0 items-center gap-1.5 overflow-hidden whitespace-nowrap">
                        {isTitle ? (
                          <>
                            <button type="button" onMouseDown={(e) => { e.stopPropagation(); }} onClick={() => { onOpen(row.rowNumber); }} className="min-w-0 truncate text-left font-medium text-pt-ink hover:text-pt-primary hover:underline">{v === '' ? '（名前なし）' : v}</button>
                            <ChangeMark kind={row.changedSinceLastView} />
                            <button
                              type="button"
                              aria-label={`${v}さんの詳細を開く`}
                              title="詳細を開く"
                              onMouseDown={(e) => { e.stopPropagation(); }}
                              onClick={() => { onOpen(row.rowNumber); }}
                              className={cx('ml-auto inline-flex h-6 w-6 shrink-0 items-center justify-center rounded text-pt-muted hover:bg-pt-surface hover:text-pt-ink hover:shadow-pt-card focus-visible:opacity-100', rowSelected ? 'opacity-100' : 'opacity-0 group-hover:opacity-100')}
                            >
                              <IconExpand size={13} />
                            </button>
                          </>
                        ) : c.header === sheet.statusColumn ? (
                          <StatusPill value={v} dot={statusDot(sheet, v)} />
                        ) : (
                          <CellDisplay column={c} value={v} className={cx('truncate', c.multiline && 'text-pt-muted')} />
                        )}
                      </div>
                      {st?.kind === 'conflict' && !isSel && <span className="pointer-events-none absolute inset-0 ring-1 ring-pt-warn ring-inset" />}
                      {st?.kind === 'failed' && !isSel && <span className="pointer-events-none absolute inset-0 ring-1 ring-pt-danger ring-inset" />}
                      {isSel && !isEdit && <span className="pointer-events-none absolute inset-0 z-[1] ring-2 ring-pt-select ring-inset" />}
                      <CellMark state={st} />
                      {isEdit && editing.mode === 'widget' && (
                        <Editor
                          col={c}
                          initial={editing.value}
                          wide={kindOf(c) === 'datetime'}
                          place={fitInBox('below')}
                          onChange={setEditValue}
                          onCommit={commit}
                          onCancel={cancel}
                        />
                      )}
                      {isEdit && editing.mode === 'choice' && (
                        <ChoiceMenu col={c} value={editing.base} query={editing.query} place={fitInBox('below')} onPick={(o) => { commit(o, null); }} onCancel={cancel} />
                      )}
                      {saver.bubble === key && st && (st.kind === 'conflict' || st.kind === 'failed') && (
                        <div data-bubble ref={fitInBox('right')} className="absolute top-0 left-full z-40 ml-1 w-72 rounded-lg border border-pt-line bg-pt-surface p-3 whitespace-normal shadow-pt-pop" onMouseDown={(e) => { e.stopPropagation(); }}>
                          <SaveProblem
                            state={st}
                            display={(x) => (x === '' ? '（空）' : x)}
                            onTakeSheet={() => { saver.takeSheet(key); focusGrid(); }}
                            onOverwrite={() => { saver.overwrite(key, labelFor(row.rowNumber, c.header)); focusGrid(); }}
                            onRetry={() => { saver.retry(key, labelFor(row.rowNumber, c.header)); focusGrid(); }}
                            onClose={() => { saver.setBubble(null); focusGrid(); }}
                          />
                        </div>
                      )}
                    </td>
                  );
                })}
              </tr>
            );
          })}
        </tbody>
      </table>
      <textarea
        ref={catcherRef}
        aria-label={textEditing && editingCol ? labelFor(textEditing.rowNumber, textEditing.column) : '応募者の表'}
        value={textEditing ? textEditing.value : idleText}
        onChange={(e) => {
          const v = e.target.value;
          if (editingRef.current?.mode === 'text') { setEditValue(v); return; }
          if (composingRef.current) { setIdleText(v); return; }
          typed(v);
        }}
        onCompositionStart={() => {
          composingRef.current = true;
          if (editingRef.current || !sel) return;
          const col = cols.find((c) => c.header === sel.column);
          if (col && col.header !== titleHeader && col.writable && !readOnly && modeOf(col) === 'text') startEdit(sel, { initial: '' });
        }}
        onCompositionEnd={(e) => {
          composingRef.current = false;
          if (editingRef.current?.mode === 'text') { setEditValue(e.currentTarget.value); return; }
          if (!editingRef.current) typed(e.currentTarget.value);
        }}
        onKeyDown={onKey}
        onBlur={() => { const ed = editingRef.current; if (ed?.mode === 'text' && !composingRef.current) commit(ed.value, null); }}
        spellCheck={false}
        autoComplete="off"
        className={cx(
          'absolute resize-none overflow-hidden text-[13px] outline-none',
          textEditing
            ? cx('z-40 rounded-sm border-2 border-pt-select bg-pt-surface px-2 text-pt-ink shadow-pt-pop', editingCol?.multiline ? 'overflow-y-auto py-1.5 leading-relaxed' : 'py-0 leading-[32px] whitespace-nowrap')
            : 'pointer-events-none z-0 border-0 bg-transparent p-0 text-transparent caret-transparent opacity-0',
        )}
        style={catcherStyle}
      />
      {textEditing && editingCol?.multiline && (
        <p className="pointer-events-none absolute z-40 rounded bg-pt-surface px-1 text-[11px] text-pt-faint" style={{ left: catcherBox.left, top: catcherBox.top + catcherBox.height + 2 }}>Ctrl+Enter で保存、Esc で取り消し</p>
      )}
      {rows.length === 0 && <p className="sticky left-0 px-4 py-10 text-center text-[13px] text-pt-muted" style={{ width: 'min(100%, 100vw)' }}>条件に合う応募者はいません</p>}
      <div className="h-40" />
    </div>
  );
}

/** 日付・日時のマスの入力欄。値は自分で持ち、確定のときに渡す。 */
function Editor({ col, initial, wide, place, onChange, onCommit, onCancel }: {
  col: ColumnConfig;
  initial: string;
  wide: boolean;
  place: (el: HTMLElement | null) => void;
  onChange: (v: string) => void;
  onCommit: (v: string, then: 'down' | 'right' | 'left' | null) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  const ref = useRef<HTMLDivElement>(null);
  const set = (v: string) => { setValue(v); onChange(v); };
  return (
    <div
      ref={(el) => { ref.current = el; place(el); }}
      className={cx('absolute top-0 left-0 z-40 rounded bg-pt-surface p-0.5 shadow-pt-pop', wide ? 'w-[248px]' : 'min-w-full')}
      onMouseDown={(e) => { e.stopPropagation(); }}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === 'Tab') { e.preventDefault(); onCommit(value, e.shiftKey ? 'left' : 'right'); }
      }}
      onBlur={(e) => {
        if (e.relatedTarget instanceof Node && ref.current?.contains(e.relatedTarget)) return;
        onCommit(value, null);
      }}
    >
      <CellInput column={col} value={value} onChange={set} onCommit={() => { onCommit(value, 'down'); }} onCancel={onCancel} compact />
    </div>
  );
}

/**
 * 選択肢の列のプルダウン。マスの下に開き、強調は今の値から始まる。
 * 上の欄で打った文字で絞り込める。マウスは実際に動かしたときだけ強調を動かす。
 */
function ChoiceMenu({ col, value, query, place, onPick, onCancel }: {
  col: ColumnConfig;
  value: string;
  query: string;
  place: (el: HTMLElement | null) => void;
  onPick: (v: string) => void;
  onCancel: () => void;
}) {
  const all = useMemo(() => {
    const o = col.options ?? [];
    return o.includes(value) ? o : [value, ...o];
  }, [col.options, value]);
  const [q, setQ] = useState(query);
  const options = useMemo(() => (q === '' ? all : all.filter((o) => o !== '' && o.includes(q))), [all, q]);
  const [hi, setHi] = useState(() => (query === '' ? Math.max(0, all.indexOf(value)) : 0));
  const mouseLive = useRef(false);
  const ref = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => { inputRef.current?.focus({ preventScroll: true }); }, []);
  useOutside(ref, onCancel);
  const changeQ = (next: string) => {
    setQ(next);
    setHi(next === '' ? Math.max(0, all.indexOf(value)) : 0);
  };
  return (
    <div
      ref={(el) => { ref.current = el; place(el); }}
      onMouseDown={(e) => { e.stopPropagation(); }}
      className="absolute top-full left-0 z-40 mt-1 w-[208px] min-w-full overflow-hidden rounded-lg border border-pt-line bg-pt-surface shadow-pt-pop"
    >
      <div className="flex items-center gap-1.5 border-b border-pt-line px-2.5">
        <IconSearch size={12} className="shrink-0 text-pt-faint" />
        <input
          ref={inputRef}
          value={q}
          onChange={(e) => { changeQ(e.target.value); }}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.nativeEvent.isComposing) return;
            if (e.key === 'ArrowDown') { e.preventDefault(); setHi((h) => Math.min(options.length - 1, h + 1)); }
            if (e.key === 'ArrowUp') { e.preventDefault(); setHi((h) => Math.max(0, h - 1)); }
            if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); const o = options[hi]; if (o !== undefined) onPick(o); }
            if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
          }}
          placeholder={labelOf(col)}
          aria-label={`${labelOf(col)}を選ぶ`}
          role="combobox"
          aria-expanded
          aria-controls="pt-choice-list"
          className="h-8 min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-pt-faint"
        />
      </div>
      <div id="pt-choice-list" role="listbox" className="max-h-64 overflow-y-auto py-1" onMouseMove={() => { mouseLive.current = true; }}>
        {options.length === 0 && <p className="px-2.5 py-2 text-[12px] text-pt-faint">合う選択肢はありません</p>}
        {options.map((o, i) => (
          <button
            key={o === '' ? '(empty)' : o}
            type="button"
            role="option"
            aria-selected={i === hi}
            onMouseEnter={() => { if (mouseLive.current) setHi(i); }}
            onClick={() => { onPick(o); }}
            className={cx('flex h-8 w-full items-center gap-2 px-2.5 text-left text-[13px]', i === hi ? 'bg-pt-sunken' : '')}
          >
            <span className="w-3.5 shrink-0 text-pt-select">{o === value && <IconCheck size={13} />}</span>
            <span className={cx('truncate', o === '' && 'text-pt-faint')}>{o === '' ? '（空にする）' : o}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function HeaderCell({ col, sticky, scrolled, sort, filter, menuOpen, onMenu, onSort, onFilter, onSettings, allRows }: {
  col: ColumnConfig;
  sticky: boolean;
  scrolled: boolean;
  sort: 'asc' | 'desc' | null;
  filter: ColumnFilter | undefined;
  menuOpen: boolean;
  onMenu: (open: boolean) => void;
  onSort: (dir: 'asc' | 'desc' | null) => void;
  onFilter: (f: ColumnFilter | null) => void;
  onSettings: (() => void) | null;
  allRows: ApplicantRow[];
}) {
  const ref = useRef<HTMLDivElement>(null);
  useOutside(ref, () => { onMenu(false); }, menuOpen);
  const next = sort === null ? 'asc' : sort === 'asc' ? 'desc' : null;
  return (
    <th
      className={cx('sticky top-0 h-[36px] border-b border-r border-pt-line bg-pt-bg px-0 text-left text-[12px] font-medium text-pt-muted', sticky ? 'left-0 z-30' : 'z-20', sticky && scrolled && 'border-r-pt-line-strong')}
      style={sticky && scrolled ? { boxShadow: SCROLLED_SHADOW } : undefined}
      aria-sort={sort === 'asc' ? 'ascending' : sort === 'desc' ? 'descending' : undefined}
    >
      <div ref={ref} className="group/h relative flex h-[36px] items-center">
        <button type="button" onClick={() => { onSort(next); }} className="flex h-full min-w-0 flex-1 items-center gap-1 pl-2.5 text-left hover:text-pt-ink" title="押すと並べ替え">
          {!col.writable && !sticky && <IconLock size={11} className="shrink-0 text-pt-faint" />}
          <span className="truncate">{labelOf(col)}</span>
          {sort === 'asc' && <IconArrowUp size={12} className="shrink-0 text-pt-ink" />}
          {sort === 'desc' && <IconArrowDown size={12} className="shrink-0 text-pt-ink" />}
        </button>
        <button
          type="button"
          aria-label={`「${labelOf(col)}」で絞り込む`}
          title="絞り込み"
          onClick={() => { onMenu(!menuOpen); }}
          className={cx('mr-1 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded hover:bg-pt-sunken', filter ? 'text-pt-select opacity-100' : 'text-pt-faint opacity-0 group-hover/h:opacity-100', menuOpen && 'opacity-100')}
        >
          <IconFilter size={12} />
        </button>
        {menuOpen && <HeaderMenu col={col} sort={sort} filter={filter} onSort={(d) => { onSort(d); onMenu(false); }} onFilter={onFilter} onSettings={onSettings} allRows={allRows} />}
      </div>
    </th>
  );
}

function HeaderMenu({ col, sort, filter, onSort, onFilter, onSettings, allRows }: {
  col: ColumnConfig;
  sort: 'asc' | 'desc' | null;
  filter: ColumnFilter | undefined;
  onSort: (dir: 'asc' | 'desc' | null) => void;
  onFilter: (f: ColumnFilter | null) => void;
  onSettings: (() => void) | null;
  allRows: ApplicantRow[];
}) {
  const counts = useMemo(() => {
    const m = new Map<string, number>();
    for (const r of allRows) { const v = r.values[col.header] ?? ''; m.set(v, (m.get(v) ?? 0) + 1); }
    return m;
  }, [allRows, col.header]);
  const isChoice = col.options !== null && col.options.length > 0;
  const asList = isChoice || (!col.multiline && counts.size <= CHECK_LIST_MAX);
  const values = useMemo(() => {
    const o = col.options ?? [];
    const extra = [...counts.keys()].filter((v) => !o.includes(v)).sort((a, b) => (counts.get(b) ?? 0) - (counts.get(a) ?? 0) || a.localeCompare(b, 'ja'));
    return [...o, ...extra];
  }, [col.options, counts]);
  const checked = filter?.type === 'in' ? new Set(filter.values) : new Set(values);
  const toggle = (v: string) => {
    const s = new Set(checked);
    if (s.has(v)) s.delete(v); else s.add(v);
    onFilter(s.size === values.length ? null : { type: 'in', values: [...s] });
  };
  return (
    <div className="absolute top-full left-0 z-40 mt-1 w-60 rounded-lg border border-pt-line bg-pt-surface py-1 font-normal text-pt-ink shadow-pt-pop">
      <MenuItem active={sort === 'asc'} onClick={() => { onSort(sort === 'asc' ? null : 'asc'); }}><IconArrowUp size={13} />昇順に並べる</MenuItem>
      <MenuItem active={sort === 'desc'} onClick={() => { onSort(sort === 'desc' ? null : 'desc'); }}><IconArrowDown size={13} />降順に並べる</MenuItem>
      <div className="my-1 border-t border-pt-line" />
      <div className="flex items-center px-3 pt-1 pb-1.5">
        <p className="text-[11px] font-medium text-pt-faint">絞り込み</p>
        {asList && (
          <span className="ml-auto flex gap-2 text-[11px]">
            <button type="button" onClick={() => { onFilter(null); }} className="text-pt-primary hover:underline">すべて</button>
            <button type="button" onClick={() => { onFilter({ type: 'in', values: [] }); }} className="text-pt-muted hover:underline">外す</button>
          </span>
        )}
      </div>
      {asList ? (
        <div className="max-h-64 overflow-y-auto pb-1">
          {values.map((v) => (
            <label key={v === '' ? '(empty)' : v} className="flex h-8 cursor-pointer items-center gap-2 px-3 text-[13px] hover:bg-pt-sunken">
              <input type="checkbox" checked={checked.has(v)} onChange={() => { toggle(v); }} className="h-3.5 w-3.5 accent-pt-select" />
              <span className={cx('min-w-0 flex-1 truncate', v === '' && 'text-pt-faint')}>{v === '' ? '（空）' : v}</span>
              <span className="pt-num text-[12px] text-pt-faint">{counts.get(v) ?? 0}</span>
            </label>
          ))}
        </div>
      ) : (
        <div className="px-3 pb-2">
          <input
            autoFocus
            value={filter?.type === 'has' ? filter.text : ''}
            onChange={(e) => { onFilter(e.target.value === '' ? null : { type: 'has', text: e.target.value }); }}
            placeholder="含む文字"
            className="h-8 w-full rounded-md border border-pt-line-strong px-2 text-[13px] outline-none focus:border-pt-select focus:ring-2 focus:ring-pt-select/20"
          />
        </div>
      )}
      {filter && (
        <>
          <div className="my-1 border-t border-pt-line" />
          <MenuItem onClick={() => { onFilter(null); }}>絞り込みを外す</MenuItem>
        </>
      )}
      {onSettings && (
        <>
          <div className="my-1 border-t border-pt-line" />
          <MenuItem onClick={onSettings}><IconColumns size={13} />この項目の設定</MenuItem>
        </>
      )}
    </div>
  );
}

function MenuItem({ active = false, onClick, children }: { active?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onClick} className={cx('flex h-8 w-full items-center gap-2 px-3 text-left text-[13px] hover:bg-pt-sunken', active ? 'text-pt-select' : 'text-pt-ink')}>
      {children}
    </button>
  );
}
