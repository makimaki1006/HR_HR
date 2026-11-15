import type { ReactNode } from 'react';

/*
 * 押せるものの見た目は、押すと何が起きるかで 4 通りに分ける（案件管理の全画面で共通）。
 * - 別の画面へ移る: GoLink（青い字に下線、「→」付き）
 * - 小窓・詳細・一覧が開く: 文字付きのボタン（「詳細 ▸」「一覧を見る ▾」）。className="jc-open"
 * - 外へ出る: OutLink（「↗ HubSpot」）
 * - 一覧を絞る・開く切り替え: CountChip（件数付きの丸い札）
 * 見た目は home.css の「押せるものの 4 通り」。
 */

/** 別の画面へ移る。 */
export function GoLink({ children, onClick, className = '' }: { children: ReactNode; onClick: () => void; className?: string }) {
  return <button type="button" className={`jc-go ${className}`.trim()} onClick={onClick}><span aria-hidden="true">→ </span>{children}</button>;
}

/** 外へ出る（新しいタブで HubSpot などを開く）。 */
export function OutLink({ href, label = 'HubSpot', name }: { href: string; label?: string; name?: string }) {
  return <a className="jc-out" href={href} target="_blank" rel="noreferrer" aria-label={name}><span aria-hidden="true">↗ </span>{label}</a>;
}

/** 一覧を開く・閉じる切り替えの札。0 件も薄く残して並びを変えない。 */
export function CountChip({ label, count, pressed, onClick }: { label: string; count: number | string; pressed: boolean; onClick: () => void }) {
  const zero = count === 0;
  return <button type="button" className={`jc-chip${zero ? ' is-zero' : ''}`} aria-pressed={pressed} onClick={onClick}>{label} <span className="jc-chip-count">{typeof count === 'number' ? `${String(count)}件` : count}</span></button>;
}

/** カードの右下に出す「一覧を見る ▾」（開いているときは「閉じる ▴」）。カード自体がボタンなので、ここは見た目の手がかりだけ。 */
export function CardOpenCue({ open }: { open: boolean }) {
  return <span className="jc-open jh-card-cue" aria-hidden="true">{open ? '閉じる ▴' : '一覧を見る ▾'}</span>;
}

/** 開いた一覧の見出しの右に置く「× 閉じる」。 */
export function CloseListButton({ onClick, label }: { onClick: () => void; label: string }) {
  return <button type="button" className="jc-open jh-list-close" aria-label={`${label}を閉じる`} onClick={onClick}><span aria-hidden="true">× </span>閉じる</button>;
}
