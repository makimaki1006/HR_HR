// 1 マスの表示と入力欄。一覧の表（その場で編集）と応募者の詳細の両方で使う。
// シートの値は文字のまま持つ。日付は "2026/10/24"、日時は "2026/10/24 14:00"。
import { useEffect, useRef, type KeyboardEvent } from 'react';
import { kindOf } from '../columns';
import type { ColumnConfig } from '../types';
import { cx } from './kit';

const toIsoDate = (v: string): string => {
  const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})/.exec(v);
  return m ? `${m[1] ?? ''}-${(m[2] ?? '').padStart(2, '0')}-${(m[3] ?? '').padStart(2, '0')}` : '';
};
const toIsoTime = (v: string): string => {
  const m = /(\d{1,2}):(\d{2})$/.exec(v);
  return m ? `${(m[1] ?? '').padStart(2, '0')}:${m[2] ?? ''}` : '';
};
const fromIsoDate = (d: string): string => (d ? d.replace(/-/g, '/') : '');

/** マスの表示。電話・メールは押せる。空は薄い横線。 */
export function CellDisplay({ column, value, className }: { column: ColumnConfig; value: string; className?: string }) {
  if (value === '') return <span className={cx('text-pt-faint', className)}>—</span>;
  const kind = kindOf(column);
  if (kind === 'phone') return <a href={`tel:${value.replace(/[^\d+]/g, '')}`} className={cx('pt-num text-pt-primary hover:underline', className)} onClick={(e) => { e.stopPropagation(); }}>{value}</a>;
  if (kind === 'email') return <a href={`mailto:${value}`} className={cx('text-pt-primary hover:underline', className)} onClick={(e) => { e.stopPropagation(); }}>{value}</a>;
  if (kind === 'date' || kind === 'datetime') return <span className={cx('pt-num', className)}>{value}</span>;
  return <span className={className}>{value}</span>;
}

/**
 * マスの入力欄。Enter（複数行は Ctrl+Enter）で確定、Esc で取り消し。
 * 選択肢はシートの入力規則と同じものを出す。
 */
export function CellInput({ column, value, onChange, onCommit, onCancel, autoFocus = true, compact = false }: {
  column: ColumnConfig;
  value: string;
  onChange: (v: string) => void;
  onCommit: () => void;
  onCancel: () => void;
  autoFocus?: boolean;
  /** 表のマスの中に収める小さい形。 */
  compact?: boolean;
}) {
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
        {!column.options.includes(value) && <option value={value}>{value === '' ? '（空）' : value}</option>}
        {column.options.map((o) => <option key={o} value={o}>{o === '' ? '（空）' : o}</option>)}
      </select>
    );
  }
  if (kind === 'date') {
    return <input ref={ref} type="date" value={toIsoDate(value)} onChange={(e) => { onChange(fromIsoDate(e.target.value)); }} onKeyDown={onKey} className={base} />;
  }
  if (kind === 'datetime') {
    const d = toIsoDate(value);
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
