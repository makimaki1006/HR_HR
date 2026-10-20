// 顧客向け画面の共通部品。各画面はここを使い、見た目をそろえる。
import { NOW } from '../fixtures';
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes, TextareaHTMLAttributes } from 'react';

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(' ');

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';

export function Button({ variant = 'secondary', size = 'md', className, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; size?: 'sm' | 'md' | 'lg' }) {
  return (
    <button
      type="button"
      {...rest}
      className={cx(
        'inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50',
        size === 'sm' && 'h-8 px-3 text-sm',
        size === 'md' && 'h-10 px-4 text-sm',
        size === 'lg' && 'h-12 px-5 text-base',
        variant === 'primary' && 'bg-pt-primary text-white hover:bg-pt-primary-hover',
        variant === 'secondary' && 'border border-pt-line-strong bg-pt-surface text-pt-ink hover:bg-pt-bg',
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

const inputBase = 'w-full rounded-md border border-pt-line-strong bg-pt-surface px-3 text-base text-pt-ink placeholder:text-pt-faint focus:border-pt-primary focus:outline-none focus:ring-2 focus:ring-pt-primary-soft disabled:bg-pt-bg disabled:text-pt-muted';

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return <input {...rest} className={cx(inputBase, 'h-11', className)} />;
}

export function TextArea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...rest} className={cx(inputBase, 'min-h-24 py-2 leading-relaxed', className)} />;
}

export function Select({ className, children, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...rest} className={cx(inputBase, 'h-11', className)}>{children}</select>;
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
        'inline-flex items-center rounded px-1.5 py-0.5 text-xs font-medium whitespace-nowrap',
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
  return <div className={cx('rounded-lg border border-pt-line bg-pt-surface', className)}>{children}</div>;
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

export { cx };
