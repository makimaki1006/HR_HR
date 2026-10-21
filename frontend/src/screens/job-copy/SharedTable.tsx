import { sortArrow } from './sharedTableModel';

/** 並べ替えできる見出し。選んでいる列は aria-sort と矢印で向きを示す。 */
export function SortHeader({ label, active, descending, onSort }: { label: string; active: boolean; descending: boolean; onSort: () => void }) {
  return <th scope="col" aria-sort={active ? descending ? 'descending' : 'ascending' : 'none'}><button type="button" onClick={onSort}>{label}{sortArrow(active, descending)}</button></th>;
}

/** 前の／次のページ送り。真ん中の表示は呼び出し側が決める（「1 / 2」「1〜20 / 45件」など）。 */
export function PageNav({ className, label, size, hasPrev, hasNext, onPrev, onNext, children }: { className: string; label: string; size: number; hasPrev: boolean; hasNext: boolean; onPrev: () => void; onNext: () => void; children: React.ReactNode }) {
  return <nav className={className} aria-label={label}><button type="button" className="jc-button" disabled={!hasPrev} onClick={onPrev}>前の{size}件</button><span>{children}</span><button type="button" className="jc-button" disabled={!hasNext} onClick={onNext}>次の{size}件</button></nav>;
}
