import { useEffect, useId, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
import { loadHolidays } from './holidays';
import { addDays, addMonths, displayDate, eraYear, monthGrid, pad, parseDate, splitIso, todayIso, weekdayOf } from './datePickerModel';
import './date-picker.css';

const WEEK = ['日', '月', '火', '水', '木', '金', '土'] as const;

export interface DatePickerProps {
  /** YYYY-MM-DD (空 = 未入力) */
  value: string;
  onChange: (value: string) => void;
  id?: string | undefined;
  'aria-label'?: string | undefined;
  'aria-invalid'?: boolean | undefined;
  'aria-describedby'?: string | undefined;
  min?: string | undefined;
  max?: string | undefined;
  autoFocus?: boolean | undefined;
  /** 入力欄のキー操作 (カレンダーが閉じているとき Esc / Enter などを親へ渡す) */
  onKeyDown?: ((e: KeyboardEvent) => void) | undefined;
  onBlur?: (() => void) | undefined;
  /** テスト・基準日の差し替え用 (既定は今日) */
  today?: string | undefined;
}

/** 土曜 = 青 / 日曜・祝日 = 赤 を文字色で示す日付入力。値は YYYY-MM-DD の文字列で受け渡す */
export function DatePicker(p: DatePickerProps) {
  const { value, onChange, min, max } = p;
  const today = p.today ?? todayIso();
  const [text, setText] = useState(displayDate(value));
  const [open, setOpen] = useState(false);
  const [focusDate, setFocusDate] = useState(today);
  const [view, setView] = useState(() => { const s = splitIso(parseDate(value) ?? today); return { y: s.y, m: s.m }; });
  const [holidays, setHolidays] = useState<Record<string, string>>({});
  const root = useRef<HTMLSpanElement | null>(null);
  const input = useRef<HTMLInputElement | null>(null);
  const grid = useRef<HTMLTableElement | null>(null);
  const focusWanted = useRef(false);
  const popId = useId();

  // 親から値が変わったら、入力欄の表示を合わせる (入力途中の文字が同じ日付を指しているときは触らない)
  const [seen, setSeen] = useState(value);
  if (seen !== value) {
    setSeen(value);
    if (parseDate(text) !== value) setText(displayDate(value));
  }

  const cells = useMemo(() => monthGrid(view.y, view.m), [view.y, view.m]);

  // 祝日は開いたときに、表示中の月に必要な年だけ読む
  useEffect(() => {
    if (!open) return;
    let live = true;
    const years = [splitIso(cells[0] ?? today).y, splitIso(cells[41] ?? today).y];
    void loadHolidays(years).then(h => { if (live) setHolidays(prev => ({ ...prev, ...h })); });
    return () => { live = false; };
  }, [open, cells, today]);

  const inRange = (iso: string): boolean => (min === undefined || min === '' || iso >= min) && (max === undefined || max === '' || iso <= max);

  const tabbable = (() => {
    const f = splitIso(focusDate);
    if (f.y === view.y && f.m === view.m) return focusDate;
    return cells.find(c => { const s = splitIso(c); return s.y === view.y && s.m === view.m; }) ?? focusDate;
  })();

  // キーボードで動かしている間は、位置にあるボタンへフォーカスする
  useEffect(() => {
    if (!open || !focusWanted.current) return;
    grid.current?.querySelector<HTMLButtonElement>(`button[data-date="${tabbable}"]`)?.focus();
  }, [open, tabbable, view]);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (root.current !== null && !root.current.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', away);
    return () => { document.removeEventListener('mousedown', away); };
  }, [open]);

  function openCalendar() {
    const base = parseDate(value) ?? (min !== undefined && min !== '' && min > today ? min : today);
    setView(splitIso(base));
    setFocusDate(base);
    focusWanted.current = true;
    setOpen(true);
  }
  function close() {
    focusWanted.current = false;
    setOpen(false);
    input.current?.focus();
  }
  function pick(iso: string) {
    if (!inRange(iso)) return;
    setText(displayDate(iso));
    onChange(iso);
    close();
  }
  function move(next: string) {
    setFocusDate(next);
    const s = splitIso(next);
    setView({ y: s.y, m: s.m });
  }
  function shiftMonth(n: number) {
    focusWanted.current = false;
    setView(v => { const s = splitIso(addMonths(`${String(v.y)}-${pad(v.m)}-01`, n)); return { y: s.y, m: s.m }; });
  }

  function typed(raw: string) {
    setText(raw);
    if (raw.trim() === '') { onChange(''); return; }
    const iso = parseDate(raw);
    if (iso !== null) onChange(iso);
  }

  function inputKey(e: KeyboardEvent) {
    if (open && e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); return; }
    if (e.key === 'ArrowDown' && e.altKey) { e.preventDefault(); openCalendar(); return; }
    p.onKeyDown?.(e);
  }

  function gridKey(e: KeyboardEvent) {
    const steps: Record<string, () => string> = {
      ArrowLeft: () => addDays(tabbable, -1), ArrowRight: () => addDays(tabbable, 1),
      ArrowUp: () => addDays(tabbable, -7), ArrowDown: () => addDays(tabbable, 7),
      PageUp: () => addMonths(tabbable, -1), PageDown: () => addMonths(tabbable, 1),
    };
    const step = steps[e.key];
    if (step !== undefined) { e.preventDefault(); e.stopPropagation(); focusWanted.current = true; move(step()); return; }
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
  }

  const parsed = parseDate(value);
  const era = eraYear(view.y);

  return <span className="dp" ref={root}>
    <span className="dp-field">
      <input ref={input} id={p.id} type="text" inputMode="numeric" autoComplete="off" placeholder="YYYY/MM/DD" maxLength={10}
        aria-label={p['aria-label']} aria-invalid={p['aria-invalid']} aria-describedby={p['aria-describedby']}
        autoFocus={p.autoFocus} value={text}
        onChange={e => { typed(e.target.value); }} onKeyDown={inputKey}
        onBlur={() => {
          if (parseDate(text) === null && text.trim() !== '') setText(displayDate(value));
          else if (parsed !== null) setText(displayDate(parsed));
          p.onBlur?.();
        }} />
      <button type="button" className="dp-open" aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? popId : undefined}
        aria-label="カレンダーを開く" onClick={() => { if (open) close(); else openCalendar(); }}>
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3 2v2M13 2v2M2 5h12M3 3h10a1 1 0 011 1v9a1 1 0 01-1 1H3a1 1 0 01-1-1V4a1 1 0 011-1z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" /></svg>
      </button>
    </span>
    {open && <div className="dp-pop" id={popId} role="dialog" aria-label="日付を選ぶ" data-testid="dp-popover">
      <div className="dp-head">
        <button type="button" aria-label="前の月" onClick={() => { shiftMonth(-1); }}>‹</button>
        <strong aria-live="polite">{`${String(view.y)}年${era !== '' ? `(${era})` : ''} ${String(view.m)}月`}</strong>
        <button type="button" aria-label="次の月" onClick={() => { shiftMonth(1); }}>›</button>
      </div>
      <table className="dp-grid" role="grid" ref={grid} onKeyDown={gridKey} aria-label={`${String(view.y)}年${String(view.m)}月`}>
        <thead><tr>{WEEK.map((w, i) => <th key={w} scope="col" className={i === 0 ? 'dp-sun' : i === 6 ? 'dp-sat' : ''}>{w}</th>)}</tr></thead>
        <tbody>{Array.from({ length: 6 }, (_, r) => <tr key={r}>{cells.slice(r * 7, r * 7 + 7).map(iso => {
          const s = splitIso(iso);
          const name = holidays[iso];
          const dow = weekdayOf(iso);
          const disabled = !inRange(iso);
          const cls = ['dp-day',
            s.m !== view.m ? 'dp-out' : '',
            name !== undefined ? 'dp-holiday' : dow === 0 ? 'dp-sun' : dow === 6 ? 'dp-sat' : '',
            iso === today ? 'dp-today' : '', iso === parsed ? 'dp-selected' : '', disabled ? 'dp-disabled' : ''].filter(Boolean).join(' ');
          const label = name !== undefined ? `${String(s.m)}月${String(s.d)}日 ${name}` : `${String(s.m)}月${String(s.d)}日(${WEEK[dow] ?? ''})`;
          return <td key={iso} role="gridcell" aria-selected={iso === parsed}>
            <button type="button" className={cls} data-date={iso} data-holiday={name} aria-label={label} title={name !== undefined ? label : undefined}
              aria-disabled={disabled || undefined} tabIndex={iso === tabbable ? 0 : -1} aria-current={iso === today ? 'date' : undefined}
              onClick={() => { pick(iso); }}>{s.d}</button>
          </td>;
        })}</tr>)}</tbody>
      </table>
      <div className="dp-foot">
        <button type="button" disabled={!inRange(today)} onClick={() => { pick(today); }}>今日</button>
        <button type="button" onClick={() => { setText(''); onChange(''); close(); }}>削除</button>
      </div>
    </div>}
  </span>;
}
