// スマホの「状況」を選ぶ下からのシートの中身。行は 52px・文字 15px。今の状況より先の段階を「次によく使う」として先頭に出す。
import { useMemo } from 'react';
import { stageOf } from '../../data';
import { IconCheck } from '../../ui/icons';
import { cx } from '../../ui/kit';
import type { FunnelStage, SheetConfig } from '../../types';
import { statusDot } from './model';

const STAGE_ORDER: FunnelStage[] = ['applied', 'screening', 'interview_set', 'interviewed', 'offer', 'hired', 'rejected', 'declined'];

/** 今の値の段階より先の段階の選択肢（段階の順、同じ段階はシートの並び）を最大 3 つ。段階の対応が無い・今の値の段階が分からない・不採用や辞退で終わっているときは出さない。 */
export function suggestedStatuses(sheet: SheetConfig, options: string[], current: string): string[] {
  if (!sheet.stageMap) return [];
  const cur = stageOf(sheet, current);
  if (cur === undefined || cur === 'rejected' || cur === 'declined') return [];
  const curIdx = STAGE_ORDER.indexOf(cur);
  return options
    .map((o, i) => ({ o, i, idx: o === '' || o === current ? -1 : STAGE_ORDER.indexOf(stageOf(sheet, o) ?? cur) }))
    .filter((x) => x.idx > curIdx)
    .sort((a, b) => a.idx - b.idx || a.i - b.i)
    .slice(0, 3)
    .map((x) => x.o);
}

export function StatusSheetList({ sheet, options, value, onSelect }: {
  sheet: SheetConfig;
  options: string[];
  value: string;
  onSelect: (v: string) => void;
}) {
  const suggested = useMemo(() => suggestedStatuses(sheet, options, value), [sheet, options, value]);
  const item = (o: string) => (
    <li key={o === '' ? '(empty)' : o}>
      <button
        type="button"
        role="option"
        aria-selected={o === value}
        onClick={() => { onSelect(o); }}
        className="flex h-[52px] w-full items-center gap-3 px-4 text-left text-[15px] hover:bg-pt-sunken"
      >
        <span className={cx('h-2 w-2 shrink-0 rounded-full', statusDot(sheet, o))} />
        <span className={cx('min-w-0 flex-1 truncate', o === '' && 'text-pt-faint')}>{o === '' ? '未入力' : o}</span>
        {o === value && <span className="text-pt-primary"><IconCheck size={16} /></span>}
      </button>
    </li>
  );
  return (
    <div role="listbox">
      {suggested.length > 0 && (
        <>
          <p className="px-4 pt-1 pb-1 text-[13px] text-pt-muted">次によく使う</p>
          <ul className="border-b border-pt-line pb-1">{suggested.map(item)}</ul>
          <p className="px-4 pt-3 pb-1 text-[13px] text-pt-muted">すべて</p>
        </>
      )}
      <ul>{options.map(item)}</ul>
    </div>
  );
}
