// 一覧の上の「今日」の帯、並べ替えの選択、はみ出すタブの手がかり。PC とスマホで同じものを使う。
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { cx } from '../../ui/kit';
import type { ApplicantRow, ListSort, SheetConfig } from '../../types';
import { dayLabel, TODAY_NUM } from '../../columns';
import { IconSort } from '../../ui/icons';
import { dayOffset, rowRefDay } from './model';

export type Quick = 'today' | 'tomorrow' | 'new' | 'unhandled';

const interviewDay = (sheet: SheetConfig, row: ApplicantRow, rows: ApplicantRow[] = [row]): number | null => {
  if (!sheet.interviewColumn) return null;
  const ref = sheet.sortColumn ? rowRefDay(sheet, row, rows) : null;
  return dayOffset(row.values[sheet.interviewColumn] ?? '', ref);
};

/** 未対応の人か（選考状況が「応募」の段階）。段階が決まっていないシートでは数えない。 */
const isUnhandled = (sheet: SheetConfig, row: ApplicantRow): boolean => {
  const col = sheet.statusColumn;
  return col !== undefined && sheet.stageMap?.[row.values[col] ?? ''] === 'applied';
};

export function matchesQuick(sheet: SheetConfig, row: ApplicantRow, q: Quick, rows: ApplicantRow[] = [row]): boolean {
  if (q === 'today') return interviewDay(sheet, row, rows) === 0;
  if (q === 'tomorrow') return interviewDay(sheet, row, rows) === 1;
  if (q === 'new') return row.changedSinceLastView === 'new';
  return isUnhandled(sheet, row);
}

/** 面接の日の通し番号（今日 = TODAY_NUM）。読めなければ null。 */
export function interviewDayNum(sheet: SheetConfig, row: ApplicantRow, rows: ApplicantRow[] = [row]): number | null {
  const off = interviewDay(sheet, row, rows);
  return off === null ? null : off + TODAY_NUM;
}

/** 「今日の面接はありません。次は 10/23（金）に 2 件」。今日・明日が 0 件のときに出す。次の予定が無ければ文の前半だけ。 */
export function noInterviewNote(sheet: SheetConfig, rows: ApplicantRow[], which: 'today' | 'tomorrow'): string {
  const from = which === 'today' ? 0 : 1;
  const days = rows.map((r) => interviewDayNum(sheet, r, rows)).filter((n): n is number => n !== null && n > TODAY_NUM + from);
  const head = which === 'today' ? '今日の面接はありません。' : '明日の面接はありません。';
  if (days.length === 0) return head;
  const next = Math.min(...days);
  return `${head}次は ${dayLabel(next)}に ${String(days.filter((n) => n === next).length)} 件`;
}

/** 帯に出すチップ。面接の列が決まっていないシートでは面接の 2 つを出さない。 */
export function quickChips(sheet: SheetConfig, rows: ApplicantRow[]): { id: Quick; label: string; count: number }[] {
  const count = (q: Quick): number => rows.filter((r) => matchesQuick(sheet, r, q, rows)).length;
  const out: { id: Quick; label: string; count: number }[] = [];
  if (sheet.interviewColumn) {
    out.push({ id: 'today', label: '今日の面接', count: count('today') });
    out.push({ id: 'tomorrow', label: '明日', count: count('tomorrow') });
  }
  out.push({ id: 'new', label: '新着', count: count('new') });
  if (sheet.stageMap && sheet.statusColumn) out.push({ id: 'unhandled', label: '未対応', count: count('unhandled') });
  return out;
}

const CHIP_TITLE: Record<Quick, string> = {
  today: '面接予定日が今日の人',
  tomorrow: '面接予定日が明日の人',
  new: '前回見たあとに増えた応募者',
  unhandled: '状況がまだ応募の段階の人（未連絡など）',
};

export function TodayBand({ chips, active, onPick, className }: {
  chips: { id: Quick; label: string; count: number }[];
  active: Quick | null;
  onPick: (q: Quick | null) => void;
  className?: string;
}) {
  if (chips.length === 0) return null;
  return (
    <div className={cx('flex items-center gap-1.5 overflow-x-auto', className)} role="group" aria-label="今日の用事">
      {chips.map((c) => {
        const on = active === c.id;
        return (
          <button
            key={c.id}
            type="button"
            aria-pressed={on}
            title={CHIP_TITLE[c.id]}
            onClick={() => { onPick(on ? null : c.id); }}
            className={cx(
              'inline-flex h-[32px] min-w-0 shrink-0 items-center gap-1.5 rounded-md border px-3 text-[13px] whitespace-nowrap lg:h-7 lg:px-2.5',
              c.count === 0 && !on && 'opacity-50',
              on ? 'border-pt-select bg-pt-primary-soft font-medium text-pt-primary' : 'border-pt-line bg-pt-surface text-pt-ink hover:bg-pt-sunken',
              c.count === 0 && !on && 'text-pt-muted',
            )}
          >
            {c.label}
            <span className={cx('pt-num text-[13px] font-semibold', c.count === 0 && !on ? 'text-pt-faint' : 'text-pt-ink', on && 'text-pt-primary')}>{c.count}</span>
          </button>
        );
      })}
    </div>
  );
}

type SortChoice = 'default' | 'near' | 'changed' | 'custom';

/** 並べ替え。見出しで並べているときは「見出しの並べ替え」と出る。 */
export function SortSelect({ sort, interviewColumn, onChange, className, compact = false }: {
  sort: ListSort | null;
  interviewColumn: string | undefined;
  onChange: (s: ListSort | null) => void;
  className?: string;
  /** スマホ用。「絵＋並び」の見た目にして、押すと OS の選択肢が出る。 */
  compact?: boolean;
}) {
  const value: SortChoice = sort === null ? 'default' : sort.mode === 'near' ? 'near' : sort.mode === 'changed' ? 'changed' : 'custom';
  return (
    <label className={cx('relative block', compact && 'h-[40px] w-[72px] shrink-0 rounded-md bg-pt-surface', className)}>
      <span className="sr-only">並べ替え</span>
      {compact && (
        <span aria-hidden="true" className="pointer-events-none absolute inset-0 flex items-center justify-center gap-1 text-[13px] text-pt-ink"><IconSort size={14} className="text-pt-muted" />並び</span>
      )}
      <select
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          if (v === 'default') onChange(null);
          if (v === 'near' && interviewColumn) onChange({ column: interviewColumn, dir: 'asc', mode: 'near' });
          if (v === 'changed') onChange({ column: '', dir: 'desc', mode: 'changed' });
        }}
        className={compact
          ? 'absolute inset-0 h-full w-full cursor-pointer appearance-none rounded-md border border-pt-line-strong bg-transparent text-transparent focus:border-pt-select focus:ring-2 focus:ring-pt-select/20 focus:outline-none'
          : 'h-[44px] w-full rounded-md border border-pt-line-strong bg-pt-surface px-2.5 text-base text-pt-ink focus:border-pt-select focus:ring-2 focus:ring-pt-select/20 focus:outline-none lg:h-8 lg:text-[13px]'}
      >
        <option value="default">応募日が新しい順</option>
        {interviewColumn && <option value="near">面接日が近い順</option>}
        <option value="changed">更新が新しい順</option>
        {value === 'custom' && <option value="custom" disabled>見出しで並べ替え中</option>}
      </select>
    </label>
  );
}

/** 横に並べたタブ。右にまだ続きがあるときは、右端をぼかして「›」を出す。 */
export function TabScroller({ children, fade, label }: { children: ReactNode; fade: string; label: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [more, setMore] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const calc = () => { setMore(el.scrollLeft + el.clientWidth < el.scrollWidth - 4); };
    calc();
    el.addEventListener('scroll', calc, { passive: true });
    const ro = new ResizeObserver(calc);
    ro.observe(el);
    return () => { el.removeEventListener('scroll', calc); ro.disconnect(); };
  }, []);
  return (
    <div className="relative min-w-0">
      <div ref={ref} className="flex min-w-0 items-center gap-0.5 overflow-x-auto" role="tablist" aria-label={label}>{children}</div>
      {more && (
        <span aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-0 flex w-9 items-center justify-end pr-1 text-[18px] text-pt-muted lg:font-semibold lg:text-pt-ink" style={{ background: `linear-gradient(to left, ${fade} 45%, transparent)` }}>›</span>
      )}
    </div>
  );
}
