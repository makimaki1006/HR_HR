import { useEffect, type ReactNode } from 'react';
import type { ColumnKind, FunnelStage } from '../../types';
import { IconMail, IconPhone } from '../../ui/icons';
import { cx } from '../../ui/kit';
import { IconCalendar, IconList, IconText } from './icons';

export const KIND_ICON: Record<ColumnKind, (p: { size?: number }) => ReactNode> = {
  text: IconText,
  date: IconCalendar,
  datetime: IconCalendar,
  phone: IconPhone,
  email: IconMail,
  choice: IconList,
};

/** 開いている間だけ Esc を聞く。 */
export function useEscape(active: boolean, onEsc: () => void): void {
  useEffect(() => {
    if (!active) return undefined;
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') onEsc(); };
    window.addEventListener('keydown', h);
    return () => { window.removeEventListener('keydown', h); };
  }, [active, onEsc]);
}

export function Toggle({ checked, onChange, disabled = false, label }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => { onChange(!checked); }}
      className={cx('relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:opacity-40', checked ? 'bg-pt-primary' : 'bg-pt-line-strong')}
    >
      <span className={cx('absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all', checked ? 'left-[18px]' : 'left-0.5')} />
    </button>
  );
}

export function SettingRow({ label, hint, children }: { label: string; hint?: string | undefined; children: ReactNode }) {
  return (
    <div className="flex min-h-11 items-center gap-3 border-b border-pt-line py-1.5 last:border-b-0 sm:min-h-10">
      <div className="min-w-0 flex-1">
        <p className="text-[13px]">{label}</p>
        {hint && <p className="text-xs text-pt-faint">{hint}</p>}
      </div>
      <div className="flex max-w-[65%] shrink-0 flex-wrap justify-end gap-1">{children}</div>
    </div>
  );
}

export function Chip({ active, disabled = false, onClick, children }: { active: boolean; disabled?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      disabled={disabled}
      aria-pressed={active}
      onClick={onClick}
      className={cx('inline-flex h-8 items-center gap-1 rounded-full border px-2.5 text-xs disabled:opacity-40', active ? 'border-pt-primary bg-pt-primary-soft font-medium text-pt-primary' : 'border-pt-line text-pt-muted hover:bg-pt-sunken')}
    >
      {children}
    </button>
  );
}

const CYCLE = ['bg-pt-primary-soft', 'bg-pt-ok-soft', 'bg-pt-warn-soft', 'bg-pt-danger-soft', 'bg-pt-sunken'];
const STAGE_TONE: Partial<Record<FunnelStage, string>> = {
  applied: 'bg-pt-sunken',
  screening: 'bg-pt-primary-soft',
  interview_set: 'bg-pt-warn-soft',
  interviewed: 'bg-pt-warn-soft',
  offer: 'bg-pt-ok-soft',
  hired: 'bg-pt-ok-soft',
  rejected: 'bg-pt-danger-soft',
  declined: 'bg-pt-danger-soft',
};

export function OptionDot({ index, stage }: { index: number; stage: FunnelStage | '' }) {
  const tone = (stage !== '' ? STAGE_TONE[stage] : undefined) ?? CYCLE[index % CYCLE.length] ?? 'bg-pt-sunken';
  return <span className={cx('h-3 w-3 shrink-0 rounded-[4px] border border-pt-line-strong', tone)} aria-hidden="true" />;
}
