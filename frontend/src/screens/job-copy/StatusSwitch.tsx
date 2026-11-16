import { STATUS_LABEL } from './statusFilter';
import type { StatusCounts, StatusKey } from './statusFilter';
import './status-filter.css';

/**
 * 2〜3 択の切り替え（選んでいる方に色が付くボタンの並び）。案件管理の切り替えはこれ 1 種類にそろえる。
 * 「掲載中／掲載終了／すべて」「求人ごとに色分け／合計だけ」「すべて／稼働中だけ」など。
 */
export function ChoiceSwitch<T extends string>({ value, options, onChange, label }: { value: T; options: readonly { key: T; label: string; count?: number | null }[]; onChange: (value: T) => void; label: string }) {
  return <div className="sf-group" role="group" aria-label={label}>
    {options.map(option => <button key={option.key} type="button" className="sf-button" aria-pressed={value === option.key} onClick={() => { onChange(option.key); }}>{option.label}{option.count != null ? <span className="sf-count">{option.count.toLocaleString('ja-JP')}</span> : null}</button>)}
  </div>;
}

/** 「掲載中／掲載終了／すべて」の切り替え。掲載前・状態不明は、あるときだけ並べる。 */
export function StatusFilter({ status, counts, onChange, label = '掲載状況で絞り込む' }: { status: StatusKey; counts: StatusCounts | null; onChange: (status: StatusKey) => void; label?: string }) {
  const keys: StatusKey[] = ['live', 'ended', 'all'];
  if (counts && counts.before > 0) keys.splice(2, 0, 'before');
  if (counts && counts.unknown > 0) keys.splice(keys.length - 1, 0, 'unknown');
  return <ChoiceSwitch value={status} label={label} onChange={onChange} options={keys.map(key => ({ key, label: STATUS_LABEL[key], count: counts ? counts[key] : null }))} />;
}
