import type { KeyboardEvent, ReactNode } from 'react';

export type JobFeature = 'timeline' | 'body' | 'applications' | 'applicants' | 'reasons' | 'performance' | 'market' | 'market-table' | 'factors' | 'diff' | 'ab' | 'report' | 'receive';
interface Feature { id: JobFeature; label: string; description: string }
export const jobFeatureGroups: { id: string; label: string; features: Feature[] }[] = [
  { id: 'timeline', label: 'タイムライン', features: [{ id: 'timeline', label: 'タイムライン', description: '掲載期間・給与・本文・画像・課金・応募・市場を同じ日付の軸に並べて確認します。' }] },
  { id: 'content', label: '求人内容', features: [{ id: 'body', label: '本文・画像', description: '観測版を選び、求人本文と掲載画像を確認します。' }] },
  { id: 'applications', label: '応募分析', features: [
    { id: 'applications', label: '応募推移', description: '応募日の分かる実績を月別に確認します。' },
    { id: 'applicants', label: '応募者構成', description: '性別・年代・地域の構成と、掲載観測版の違いを確認します。' },
    { id: 'reasons', label: '応募理由', description: '応募理由・応募動機の記録を出典別に確認します。' },
    { id: 'performance', label: '課金・クリック', description: '媒体の期間別実績を確認します。未接続のデータは表示しません。' },
  ] },
  { id: 'market', label: '市場分析', features: [
    { id: 'market', label: '市場グラフ', description: '市場職種と都道府県を選び、各指標の月次推移を確認します。' },
    { id: 'market-table', label: '市場データ', description: '市場グラフと同じ職種・地域・期間の実数を確認します。' },
    { id: 'factors', label: '要因・仮説', description: '本文・画像の観測履歴と、次に確認する要因を整理します。' },
  ] },
  { id: 'comparison', label: '比較・報告', features: [
    { id: 'diff', label: '変更差分', description: '同じ求人の本文・画像の変化を比較します。' },
    { id: 'ab', label: '2求人のA/B比較', description: '異なる求人IDを組み合わせ、独立した実績を比較します。' },
    { id: 'report', label: '顧客報告・検証', description: '比較結果を整理し、次に検証する施策を記録します。' },
  ] },
  { id: 'import', label: 'データ取込', features: [{ id: 'receive', label: '外部文面を確認', description: '外部から届いた本文を現在の観測版と照合します。' }] },
];

function keyboardSelection(event: KeyboardEvent<HTMLButtonElement>, ids: string[], current: string, select: (id: string) => void, prefix: string) {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  const position = ids.indexOf(current);
  const next = event.key === 'ArrowRight' ? (position + 1) % ids.length : event.key === 'ArrowLeft' ? (position + ids.length - 1) % ids.length : event.key === 'Home' ? 0 : event.key === 'End' ? ids.length - 1 : -1;
  const id = ids[next];
  if (id === undefined) return;
  event.preventDefault();
  select(id);
  document.getElementById(`${prefix}-${id}`)?.focus({ preventScroll: true });
}

export function JobFeatureTabs({ value, onChange, remembered, prefix, children }: { value: JobFeature; onChange: (value: JobFeature) => void; remembered: Partial<Record<string, JobFeature>>; prefix: string; children: ReactNode }) {
  const group = jobFeatureGroups.find(item => item.features.some(feature => feature.id === value)) ?? jobFeatureGroups[0];
  if (!group) return null;
  const selectGroup = (id: string) => {
    const target = jobFeatureGroups.find(item => item.id === id);
    const next = remembered[id] ?? target?.features[0]?.id;
    if (next) onChange(next);
  };
  return <div className="jc-feature-workspace">
    <div className="jc-feature-primary" role="tablist" aria-label="求人管理の機能">{jobFeatureGroups.map(item => <button type="button" role="tab" id={`${prefix}-group-${item.id}`} key={item.id} aria-selected={group.id === item.id} aria-controls={`${prefix}-group-panel`} tabIndex={group.id === item.id ? 0 : -1} onClick={() => { selectGroup(item.id); }} onKeyDown={event => { keyboardSelection(event, jobFeatureGroups.map(item => item.id), item.id, selectGroup, `${prefix}-group`); }}>{item.label}</button>)}</div>
    <section className="jc-feature-group-panel" role="tabpanel" id={`${prefix}-group-panel`} aria-labelledby={`${prefix}-group-${group.id}`}>
      {jobFeatureGroups.map(item => <div className="jc-feature-secondary" role="tablist" key={item.id} aria-label={`${item.label}の表示`} hidden={group.id !== item.id}>{item.features.map(feature => <button type="button" role="tab" id={`${prefix}-feature-${feature.id}`} key={feature.id} aria-selected={value === feature.id} aria-controls={`${prefix}-panel-${feature.id}`} tabIndex={value === feature.id ? 0 : -1} onClick={() => { onChange(feature.id); }} onKeyDown={event => { keyboardSelection(event, item.features.map(feature => feature.id), feature.id, id => { const target = item.features.find(feature => feature.id === id); if (target) onChange(target.id); }, `${prefix}-feature`); }}>{feature.label}</button>)}</div>)}
      <p className="jc-feature-purpose">{group.features.find(feature => feature.id === value)?.description}</p>
      {children}
    </section>
  </div>;
}

export function JobFeaturePanel({ feature, active, prefix, children }: { feature: JobFeature; active: boolean; prefix: string; children: ReactNode }) {
  return <div className="jc-feature-panel" role="tabpanel" id={`${prefix}-panel-${feature}`} aria-labelledby={`${prefix}-feature-${feature}`} hidden={!active} tabIndex={0}>{children}</div>;
}
