import type { ReactNode } from 'react';
import { CloseListButton } from './Pressable';

/** 求人のページで手を動かす入口。札を押すと、札の下にその作業が開く（くわしい分析は横から開く）。 */
export type ListingTask = 'edit' | 'rikurozi' | 'hearing';

export interface ListingTaskOptions {
  active: ListingTask | null;
  onChange: (task: ListingTask | null) => void;
  /** 文面を手直しする。押せないときは理由。 */
  edit: { disabled: string | null; onOpen: () => void; panel: ReactNode };
  /** リクロジ向けの案を作る。押せないときは理由。 */
  rikurozi: { disabled: string | null; panel: ReactNode };
  /** くわしい分析を開く（横から開く）。 */
  onAnalysis: () => void;
}

const TITLES: Record<ListingTask | 'analysis', string> = { edit: '文面を手直しする', rikurozi: 'リクロジ向けの案を作る', hearing: '電話ヒアリングを見る', analysis: 'くわしい分析を開く' };

function Card({ title, text, note, pressed, disabled, onClick }: { title: string; text: string; note?: string | null | undefined; pressed?: boolean; disabled?: boolean; onClick: () => void }) {
  return <button type="button" className="lt-card" aria-pressed={pressed} disabled={disabled} onClick={onClick}>
    <strong>{title}</strong><span>{text}</span>{note && <small>{note}</small>}
    <span className="jc-open lt-cue" aria-hidden="true">{pressed ? '閉じる ▴' : '開く ›'}</span>
  </button>;
}

export function ListingTasks({ options, hearingSummary, hearingPanel }: { options: ListingTaskOptions; /** 「記録あり 0件 ／ 応募 3件」 */ hearingSummary: string; hearingPanel: ReactNode }) {
  const { active, onChange } = options;
  const toggle = (task: ListingTask) => { onChange(active === task ? null : task); };
  const panel = active === 'edit' ? options.edit.panel : active === 'rikurozi' ? options.rikurozi.panel : active === 'hearing' ? hearingPanel : null;
  return <section className="lt-root" aria-labelledby="lt-title">
    <h2 id="lt-title">この求人でやること</h2>
    <div className="lt-cards">
      <Card title={TITLES.edit} text="今の本文を基に直し、確認待ちの案として保存します" note={options.edit.disabled} disabled={options.edit.disabled !== null} pressed={active === 'edit'} onClick={() => { if (active !== 'edit') options.edit.onOpen(); toggle('edit'); }} />
      <Card title={TITLES.rikurozi} text="AirWork と重ならない文章を作ります（数分）" note={options.rikurozi.disabled} disabled={options.rikurozi.disabled !== null} pressed={active === 'rikurozi'} onClick={() => { toggle('rikurozi'); }} />
      <Card title={TITLES.hearing} text={hearingSummary} pressed={active === 'hearing'} onClick={() => { toggle('hearing'); }} />
      <Card title={TITLES.analysis} text="本文と画像・変更の前後・応募者など" onClick={options.onAnalysis} />
    </div>
    {active && panel && <div className="lt-panel" role="region" aria-label={TITLES[active]}>
      <div className="lt-panel-head"><h3>{TITLES[active]}</h3><CloseListButton label={TITLES[active]} onClick={() => { onChange(null); }} /></div>
      {panel}
    </div>}
  </section>;
}
