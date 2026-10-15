import { STATUS_LABEL } from './statusFilter';
import type { StatusCounts, StatusKey } from './statusFilter';
import './status-filter.css';

/** 「掲載中／掲載終了／すべて」の切り替え。掲載前・状態不明は、あるときだけ並べる。 */
export function StatusFilter({ status, counts, onChange, label = '掲載状況で絞り込む' }: { status: StatusKey; counts: StatusCounts | null; onChange: (status: StatusKey) => void; label?: string }) {
  const keys: StatusKey[] = ['live', 'ended', 'all'];
  if (counts && counts.before > 0) keys.splice(2, 0, 'before');
  if (counts && counts.unknown > 0) keys.splice(keys.length - 1, 0, 'unknown');
  return <div className="sf-group" role="group" aria-label={label}>
    {keys.map(key => <button key={key} type="button" className="sf-button" aria-pressed={status === key} onClick={() => { onChange(key); }}>{STATUS_LABEL[key]}{counts ? <span className="sf-count">{counts[key].toLocaleString('ja-JP')}</span> : null}</button>)}
  </div>;
}
