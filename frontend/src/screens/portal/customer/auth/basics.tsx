// ログイン前の画面の基本部品。試作と本番の入口で使う。サンプルのデータを読まない。
import { useState, type ReactNode } from 'react';
import { IconChevronRight } from '../../ui/icons';
import { TextInput, cx } from '../../ui/kit';

/** コピーの線の絵（icons.tsx に無いため同じ書き方で足す）。 */
export function IconCopy({ size = 14, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className={className}>
      <rect x="9" y="9" width="12" height="12" rx="2" />
      <path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1" />
    </svg>
  );
}

/** 入力欄の高さ。PC もスマホも 44px。 */
export const INPUT_LG = 'h-11 sm:h-11';
/** ボタンの高さ。PC もスマホも 44px。 */
export const BTN_LG = 'h-11 sm:h-11 w-full';

export function PasswordInput({ id, name, value, onChange, autoComplete, invalid, compact = false }: {
  id: string;
  name: string;
  value: string;
  onChange: (v: string) => void;
  autoComplete: string;
  invalid?: boolean;
  /** true なら PC で 36px（アカウントの設定画面）。 */
  compact?: boolean;
}) {
  const [shown, setShown] = useState(false);
  return (
    <div className="relative">
      <TextInput
        id={id}
        name={name}
        type={shown ? 'text' : 'password'}
        value={value}
        autoComplete={autoComplete}
        aria-invalid={invalid}
        className={cx(compact ? 'sm:h-9' : INPUT_LG, 'pr-16')}
        onChange={(e) => { onChange(e.target.value); }}
      />
      <button
        type="button"
        aria-pressed={shown}
        className="absolute right-0 top-0 h-full w-14 rounded-r-md text-[13px] text-pt-primary hover:bg-pt-bg"
        onClick={() => { setShown((s) => !s); }}
      >
        {shown ? '隠す' : '表示'}
      </button>
    </div>
  );
}

/** スマホでは高さ 44px、PC では小さめ。 */
export const TOUCH = 'h-11 sm:h-8';

export function Heading({ title, lead }: { title: string; lead?: ReactNode }) {
  return (
    <div className="mb-5">
      <h1 className="text-[17px] font-semibold text-pt-ink">{title}</h1>
      {lead && <div className="mt-1 text-[13px] leading-relaxed text-pt-muted">{lead}</div>}
    </div>
  );
}

/** 箱を使わない畳み。文字と小さな矢印だけ。 */
export function Fold({ summary, children }: { summary: string; children: ReactNode }) {
  return (
    <details className="group text-[13px]">
      <summary className="inline-flex h-9 cursor-pointer list-none items-center gap-1 text-pt-muted hover:text-pt-ink [&::-webkit-details-marker]:hidden">
        <IconChevronRight size={14} className="transition-transform group-open:rotate-90" />
        {summary}
      </summary>
      <div className="pb-1 pl-5 text-xs leading-relaxed text-pt-muted">{children}</div>
    </details>
  );
}

/** チェックボックスの行。色は pt-primary。 */
export function CheckRow({ checked, onChange, label, note }: { checked: boolean; onChange: (v: boolean) => void; label: string; note?: string }) {
  return (
    <label className="flex items-start gap-2.5 text-[13px]">
      <input type="checkbox" checked={checked} className="mt-0.5 size-4 accent-pt-primary" onChange={(e) => { onChange(e.target.checked); }} />
      <span>
        {label}
        {note && <span className="block text-xs text-pt-muted">{note}</span>}
      </span>
    </label>
  );
}

export const NETWORK_ERROR = '通信できませんでした。電波の良い場所でもう一度お試しください';
