import './home.css';
import type { Crumb, Level } from './trail';

/** 「案件管理 › 顧客 › 案件 › 求人」。ホーム・顧客・案件・求人のどの画面でも同じ場所に置く。 */
export function Breadcrumbs({ crumbs, onSelect }: { crumbs: readonly Crumb[]; onSelect: (level: Level) => void }) {
  // ホームだけ（上の階層が無い）ときは、上の帯の「案件管理」と重なるので出さない。
  if (crumbs.length < 2) return null;
  return <nav className="jh-crumbs" aria-label="現在地"><ol>{crumbs.map(crumb => <li key={crumb.level}>
    {crumb.current ? <span aria-current="page">{crumb.label}</span> : <button type="button" onClick={() => { onSelect(crumb.level); }}>{crumb.label}</button>}
  </li>)}</ol></nav>;
}
