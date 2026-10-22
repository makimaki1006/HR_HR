// 顧客向け画面の共通部品。各画面はここを使い、見た目をそろえる。
import { NOW } from '../now';
import { useEffect } from 'react';
import { IconPencil } from './icons';
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from 'react';

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(' ');

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';

export function Button({ variant = 'secondary', size = 'md', className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: 'sm' | 'md' | 'lg' }) {
  return (
    <button
      type="button"
      {...rest}
      className={cx(
        'inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-pt-select disabled:cursor-not-allowed disabled:opacity-45',
        size === 'sm' && 'h-9 px-2.5 text-[13px] sm:h-7',
        size === 'md' && 'h-11 px-3.5 text-[13px] sm:h-9',
        size === 'lg' && 'h-12 px-5 text-sm',
        variant === 'primary' && 'bg-pt-primary text-white shadow-pt-card hover:bg-pt-primary-hover',
        variant === 'secondary' && 'border border-pt-line-strong bg-pt-surface text-pt-ink shadow-pt-card hover:bg-pt-bg',
        variant === 'ghost' && 'text-pt-primary hover:bg-pt-primary-soft',
        variant === 'danger' && 'border border-pt-danger text-pt-danger bg-pt-surface hover:bg-pt-danger-soft',
        className,
      )}
    />
  );
}

export function Field({ label, hint, error, children, htmlFor }: { label: string; hint?: ReactNode; error?: ReactNode; children: ReactNode; htmlFor?: string }) {
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={htmlFor} className="text-sm font-medium text-pt-ink">{label}</label>
      {children}
      {hint && !error && <p className="text-xs text-pt-muted">{hint}</p>}
      {error && <p className="text-xs text-pt-danger">{error}</p>}
    </div>
  );
}

const inputBase = 'w-full rounded-md border border-pt-line-strong bg-pt-surface px-3 text-base sm:text-[13px] text-pt-ink placeholder:text-pt-faint focus:border-pt-primary focus:outline-none focus:ring-2 focus:ring-pt-primary-soft disabled:bg-pt-bg disabled:text-pt-muted';

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...rest} className={cx(inputBase, 'h-11 sm:h-9', className)} />;
}

export function TextArea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...rest} className={cx(inputBase, 'min-h-24 py-2 leading-relaxed', className)} />;
}

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...rest} className={cx(inputBase, 'h-11 sm:h-9', className)}>{children}</select>;
}

type Tone = 'info' | 'ok' | 'warn' | 'danger';

export function Notice({ tone = 'info', title, children, action }: { tone?: Tone; title?: ReactNode; children?: ReactNode; action?: ReactNode }) {
  return (
    <div
      role={tone === 'danger' ? 'alert' : 'status'}
      className={cx(
        'flex flex-wrap items-start gap-3 rounded-md border px-4 py-3 text-sm',
        tone === 'info' && 'border-pt-primary-soft bg-pt-primary-soft text-pt-ink',
        tone === 'ok' && 'border-pt-ok-soft bg-pt-ok-soft text-pt-ok',
        tone === 'warn' && 'border-pt-warn-soft bg-pt-warn-soft text-pt-warn',
        tone === 'danger' && 'border-pt-danger-soft bg-pt-danger-soft text-pt-danger',
      )}
    >
      <div className="min-w-0 flex-1">
        {title && <p className="font-medium">{title}</p>}
        {children && <div className={cx(title ? 'mt-0.5' : '', 'text-pt-ink')}>{children}</div>}
      </div>
      {action}
    </div>
  );
}

export function Badge({ tone = 'info', children }: { tone?: Tone | 'new' | 'neutral'; children: ReactNode }) {
  return (
    <span
      className={cx(
        'inline-flex items-center rounded-full px-2 py-0.5 text-[13px] font-medium whitespace-nowrap sm:text-[12px]',
        tone === 'new' && 'bg-pt-new text-white',
        tone === 'info' && 'bg-pt-primary-soft text-pt-primary',
        tone === 'ok' && 'bg-pt-ok-soft text-pt-ok',
        tone === 'warn' && 'bg-pt-warn-soft text-pt-warn',
        tone === 'danger' && 'bg-pt-danger-soft text-pt-danger',
        tone === 'neutral' && 'bg-pt-bg text-pt-muted border border-pt-line',
      )}
    >
      {children}
    </span>
  );
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cx('rounded-lg border border-pt-line bg-pt-surface shadow-pt-card', className)}>{children}</div>;
}

/** 日時を「10/20 14:32」の形にする（日本時間）。 */
export function formatShortDateTime(iso: string): string {
  const d = new Date(iso);
  const f = new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
  return f.format(d);
}

/** 年をまたぐ日時は年まで、同じ年なら「10/20 14:32」の形にする（日本時間）。期限の表示に使う。 */
export function formatDateTimeSmart(iso: string): string {
  const d = new Date(iso);
  const jp = (o: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', ...o }).format(d);
  const sameYear = jp({ year: 'numeric' }) === new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', year: 'numeric' }).format(new Date(NOW));
  const date = sameYear ? jp({ month: 'numeric', day: 'numeric' }) : jp({ year: 'numeric', month: 'numeric', day: 'numeric' });
  return `${date} ${jp({ hour: '2-digit', minute: '2-digit', hour12: false })}`;
}

/** 時刻を「14:32」の形にする（日本時間）。 */
export function formatTime(iso: string): string {
  return new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', hour: '2-digit', minute: '2-digit', hour12: false }).format(new Date(iso));
}

/** 絵だけのボタン。label は読み上げとツールチップに使う。 */
export function IconButton({ label, className, children, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button type="button" aria-label={label} title={label} {...rest} className={cx('inline-flex h-11 w-11 shrink-0 items-center justify-center rounded-md text-pt-muted hover:bg-pt-sunken hover:text-pt-ink disabled:opacity-40 sm:h-8 sm:w-8', className)}>
      {children}
    </button>
  );
}

/** 2〜5 個の切り替え（期間・表示の切り替えなど）。 */
export function Segmented<T extends string>({ value, options, onChange, size = 'md' }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; size?: 'sm' | 'md' }) {
  return (
    <div className="inline-flex rounded-md border border-pt-line bg-pt-sunken p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          onClick={() => { onChange(o.value); }}
          className={cx('rounded px-3 font-medium whitespace-nowrap', size === 'sm' ? 'h-9 text-xs sm:h-7' : 'h-10 text-[13px] sm:h-8', o.value === value ? 'bg-pt-surface text-pt-ink shadow-pt-card' : 'text-pt-muted hover:text-pt-ink')}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** 画面の中に出す確認・入力の枠（window.confirm の代わり）。 */
export function Dialog({ open, title, children, footer, onClose, width = 'md' }: { open: boolean; title: string; children: ReactNode; footer?: ReactNode; onClose: () => void; width?: 'sm' | 'md' | 'lg' }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[60] flex items-end justify-center bg-black/30 sm:items-center sm:p-6" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div role="dialog" aria-modal="true" aria-label={title} className={cx('flex max-h-[90dvh] w-full flex-col rounded-t-xl bg-pt-surface shadow-pt-pop sm:rounded-xl', width === 'sm' && 'sm:max-w-sm', width === 'md' && 'sm:max-w-lg', width === 'lg' && 'sm:max-w-3xl')}>
        <div className="flex items-center gap-2 border-b border-pt-line px-5 py-3">
          <h2 className="text-[15px] font-semibold">{title}</h2>
          <button type="button" onClick={onClose} className="ml-auto rounded px-2 py-1 text-sm text-pt-muted hover:bg-pt-sunken">閉じる</button>
        </div>
        <div className="overflow-y-auto px-5 py-4">{children}</div>
        {footer && <div className="flex flex-wrap justify-end gap-2 border-t border-pt-line px-5 py-3">{footer}</div>}
      </div>
    </div>
  );
}

// 注意: html の文字の大きさが 13px なので、Tailwind の rem の値（h-11 など）は 13px 基準になる（h-11 = 35.75px）。
// 決まった px が要る所（押せる所の 44px など）は h-[44px] のように px で書く。

/**
 * スマホの下からのシート。背景を押す・Esc・「閉じる」で閉じる。開いている間は後ろのスクロールを止める。
 * sm 以上では使わない（呼び出し側で PC は吹き出しなどに分ける）。
 */
export function BottomSheet({ open, title, onClose, children }: { open: boolean; title: string; onClose: () => void; children: ReactNode }) {
  useEffect(() => {
    if (!open) return undefined;
    const k = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', k);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => { document.removeEventListener('keydown', k); document.body.style.overflow = prev; };
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-[60] bg-black/30" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="pt-sheet-enter fixed inset-x-0 bottom-0 flex max-h-[70dvh] flex-col rounded-t-xl bg-pt-surface pb-[env(safe-area-inset-bottom)] shadow-pt-pop"
      >
        <div className="flex justify-center pt-2" aria-hidden="true"><span className="h-[4px] w-[36px] rounded-full bg-pt-line-strong" /></div>
        <div className="flex items-center gap-2 px-4 pt-1 pb-2">
          <h2 className="min-w-0 flex-1 truncate text-[15px] font-semibold">{title}</h2>
          <button type="button" onClick={onClose} className="-mr-2 inline-flex h-[44px] shrink-0 items-center rounded-md px-3 text-[15px] text-pt-muted hover:bg-pt-sunken">閉じる</button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain pb-2">{children}</div>
      </div>
    </div>
  );
}

/**
 * 書ける値の箱。押すと編集に入る。常に薄い枠を出し、押せると分かるようにする。
 * PC は高さ 32px で鉛筆はマウスを乗せたときだけ、スマホは高さ 44px で鉛筆を常に出す。
 */
export function EditableBox({ onActivate, children, label, className }: { onActivate: () => void; children: ReactNode; label: string; className?: string }) {
  return (
    <button
      type="button"
      onClick={onActivate}
      aria-label={`${label}を書き換える`}
      className={cx(
        'group flex min-h-[44px] w-full items-center gap-2 rounded-[6px] border border-pt-field bg-pt-surface px-[10px] py-[4px] text-left hover:border-pt-line-strong focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-pt-select sm:min-h-[32px]',
        className,
      )}
    >
      <span className="min-w-0 flex-1">{children}</span>
      <IconPencil size={12} className="shrink-0 text-pt-faint sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-visible:opacity-100" />
    </button>
  );
}

export { cx };
