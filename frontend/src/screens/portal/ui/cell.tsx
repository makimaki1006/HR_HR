// 1 マスの表示と入力欄。一覧の表（その場で編集）と応募者の詳細の両方で使う。
// シートの値は文字のまま持つ。日付は "2026/10/24"、日時は "2026/10/24 14:00"。
import { useEffect, useRef, type KeyboardEvent } from 'react';
import { formatDateTimeJa, formatDayJa, kindOf, parseLooseDay, writeDate, type DateStyle } from '../columns';
import type { ColumnConfig } from '../types';
import { cx } from './kit';

const toIsoDate = (v: string, ref?: number | null): string => {
  // 「2023年1月20日」「2022/11/9」のように書き方が混ざっていても、読めれば日付の欄に入れる。
  const n = parseLooseDay(v, ref);
  if (n === null) return '';
  return new Date(n * 86400000).toISOString().slice(0, 10);
};
const toIsoTime = (v: string): string => {
  const m = /(\d{1,2}):(\d{2})$/.exec(v);
  return m ? `${(m[1] ?? '').padStart(2, '0')}:${m[2] ?? ''}` : '';
};

/**
 * マスの表示。電話・メールは押せる。
 * 日付は「10/23（金）」の形で出す（表示だけ。原文は title に残す）。読めない値は原文を薄く出す。今日は太字。
 * 空は、書けない欄は薄い横線、empty="input" なら「入力する」。
 */
export function CellDisplay({ column, value, className, refDay, empty = 'dash' }: {
  column: ColumnConfig;
  value: string;
  className?: string;
  /** 同じ行の応募日の通し番号（年の無い日付の年を当てる）。 */
  refDay?: number | null;
  /** 空のときの出し方。'input' は書ける欄で使う。 */
  empty?: 'dash' | 'input';
}) {
  if (value === '') return <span className={cx('text-pt-faint', className)}>{empty === 'input' ? '入力する' : '—'}</span>;
  const kind = kindOf(column);
  if (kind === 'date' || kind === 'datetime') {
    const f = kind === 'date' ? formatDayJa(value, refDay) : formatDateTimeJa(value, undefined, refDay);
    if (f === null) return <span className={cx('text-pt-faint', className)}>{value}</span>;
    return <span title={value} className={cx('pt-num', f.isToday && 'font-semibold', className)}>{f.text}</span>;
  }
  if (kind === 'phone') return <a href={`tel:${value.replace(/[^\d+]/g, '')}`} className={cx('pt-num text-pt-primary hover:underline', className)} onClick={(e) => { e.stopPropagation(); }}>{value}</a>;
  if (kind === 'email') return <a href={`mailto:${value}`} className={cx('text-pt-primary hover:underline', className)} onClick={(e) => { e.stopPropagation(); }}>{value}</a>;
  return <span className={className}>{value}</span>;
}

/**
 * マスの入力欄。Enter（複数行は Ctrl+Enter）で確定、Esc で取り消し。
 * 選択肢はシートの入力規則と同じものを出す。
 */
export function CellInput({ column, value, onChange, onCommit, onCancel, autoFocus = true, compact = false, dateStyle = 'slash', refDay }: {
  column: ColumnConfig;
  value: string;
  onChange: (v: string) => void;
  onCommit: () => void;
  onCancel: () => void;
  autoFocus?: boolean;
  /** 表のマスの中に収める小さい形。 */
  compact?: boolean;
  /** 日付を書くときの書き方。呼び出し側で dominantDateStyle(rows, header) を渡す。既定は 2026/10/24。 */
  dateStyle?: DateStyle;
  /** 同じ行の応募日の通し番号（年の無い日付を入力欄に入れるときの年）。 */
  refDay?: number | null;
}) {
  const fromIsoDate = (d: string): string => writeDate(d, dateStyle);
  const ref = useRef<HTMLInputElement & HTMLTextAreaElement & HTMLSelectElement>(null);
  useEffect(() => { if (autoFocus) ref.current?.focus(); }, [autoFocus]);
  const kind = kindOf(column);
  const base = cx('w-full rounded border border-pt-select bg-pt-surface text-pt-ink outline-none ring-2 ring-pt-select/20', compact ? 'h-8 px-1.5 text-[13px]' : 'h-11 px-3 text-base sm:h-9 sm:text-[13px]');
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
    if (e.key === 'Enter' && (!column.multiline || e.ctrlKey || e.metaKey)) { e.preventDefault(); onCommit(); }
  };
  if (column.options && column.options.length > 0) {
    return (
      <select ref={ref} value={value} onChange={(e) => { onChange(e.target.value); }} onKeyDown={onKey} className={base}>
        {!column.options.includes(value) && <option value={value}>{value === '' ? '未入力' : value}</option>}
        {column.options.map((o) => <option key={o} value={o}>{o === '' ? '未入力' : o}</option>)}
      </select>
    );
  }
  if (kind === 'date') {
    return <input ref={ref} type="date" value={toIsoDate(value, refDay)} onChange={(e) => { onChange(fromIsoDate(e.target.value)); }} onKeyDown={onKey} className={base} />;
  }
  if (kind === 'datetime') {
    const d = toIsoDate(value, refDay);
    const t = toIsoTime(value);
    return (
      <div className="flex gap-1">
        <input ref={ref} type="date" value={d} onChange={(e) => { onChange(e.target.value ? `${fromIsoDate(e.target.value)} ${t || '10:00'}` : ''); }} onKeyDown={onKey} className={base} />
        <input type="time" value={t} disabled={!d} onChange={(e) => { onChange(`${fromIsoDate(d)} ${e.target.value}`); }} onKeyDown={onKey} className={cx(base, compact ? 'w-24' : 'w-32', 'disabled:opacity-50')} />
      </div>
    );
  }
  if (column.multiline) {
    return <textarea ref={ref} value={value} onChange={(e) => { onChange(e.target.value); }} onKeyDown={onKey} rows={compact ? 3 : 4} className={cx(base, 'h-auto py-1.5 leading-relaxed')} />;
  }
  return <input ref={ref} type={kind === 'email' ? 'email' : kind === 'phone' ? 'tel' : 'text'} value={value} onChange={(e) => { onChange(e.target.value); }} onKeyDown={onKey} className={base} />;
}
