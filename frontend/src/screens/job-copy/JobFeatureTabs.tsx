import type { KeyboardEvent, ReactNode } from 'react';

export type JobFeature = 'timeline' | 'body' | 'applications' | 'applicants' | 'reasons' | 'performance' | 'market' | 'market-table' | 'factors' | 'diff' | 'ab' | 'report' | 'receive';
interface Feature { id: JobFeature; label: string; description: string }
/**
 * hidden: タブの列に出さないグループ。外した分析と外部文面の確認は、URL で直接開ける（2026-10-21、ほかの分析を 13 → 6）。
 */
export const jobFeatureGroups: { id: string; label: string; hidden?: boolean; features: Feature[] }[] = [
  { id: 'timeline', label: 'タイムライン', features: [{ id: 'timeline', label: 'タイムライン', description: '掲載期間・給与・本文・画像・課金・応募・市場を同じ日付の軸に並べて確認します。' }] },
  { id: 'content', label: '本文・画像', features: [{ id: 'body', label: '本文・画像', description: '日時を選び、求人本文と掲載画像を確認します。' }] },
  { id: 'diff', label: '変更差分', features: [{ id: 'diff', label: '変更差分', description: '同じ求人の本文・画像の変化を比較します。' }] },
  { id: 'ab', label: '求人比較', features: [{ id: 'ab', label: '求人比較', description: '同じ顧客の求人や、ほかの顧客で応募が多い求人と、内容と成果を比べます。' }] },
  { id: 'applicants', label: '応募者構成', features: [{ id: 'applicants', label: '応募者構成', description: '性別・年代・地域の構成と、本文ごとの違いを確認します。' }] },
  { id: 'reasons', label: '応募理由', features: [{ id: 'reasons', label: '応募理由', description: '応募理由・応募動機の記録を出典別に確認します。' }] },
  { id: 'factors', label: '要因・仮説', features: [{ id: 'factors', label: '要因・仮説', description: '本文・画像を確認した履歴と、次に確認する要因を整理します。' }] },
  // 次の 6 つはタブに並べない。画面の URL（feature=…）で直接開ける。開いたときは見出しと「タイムラインに戻る」だけを出す。
  { id: 'applications', label: '応募推移', hidden: true, features: [{ id: 'applications', label: '応募推移', description: '応募日の分かる実績を月別に確認します。' }] },
  { id: 'performance', label: '課金・クリック', hidden: true, features: [{ id: 'performance', label: '課金・クリック', description: '媒体の期間別実績を確認します。未接続のデータは表示しません。' }] },
  { id: 'market', label: '市場グラフ', hidden: true, features: [{ id: 'market', label: '市場グラフ', description: '各指標の月次推移を確認します。市場と比べる職種は右の欄で指定します。' }] },
  { id: 'market-table', label: '市場データ', hidden: true, features: [{ id: 'market-table', label: '市場データ', description: '市場グラフと同じ職種・地域・期間の実数を確認します。' }] },
  { id: 'report', label: '顧客報告・検証', hidden: true, features: [{ id: 'report', label: '顧客報告・検証', description: '比較結果を整理し、次に検証する施策を記録します。' }] },
  { id: 'import', label: '外部文面を確認', hidden: true, features: [{ id: 'receive', label: '外部文面を確認', description: '外部から届いた本文を現在の本文と照合します。' }] },
];
/** 「ほかの分析」に出す 6 つ。タブの並びと、ページ内の入口のボタンで同じ。 */
export const jobFeatureMenu = jobFeatureGroups.filter(group => !group.hidden).flatMap(group => group.features);

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

const groupOf = (feature: JobFeature) => jobFeatureGroups.find(item => item.features.some(entry => entry.id === feature));

/** 機能パネルの名前を与える要素の id。サブタブが無いグループは上の段のタブ（隠すグループは見出し）を指す。 */
export function featureLabelId(prefix: string, feature: JobFeature): string {
  const group = groupOf(feature);
  if (group?.hidden) return `${prefix}-hidden-${group.id}`;
  return `${prefix}-group-${group?.id ?? feature}`;
}

/** 機能を開いたあと、フォーカスを移すタブ（サブタブが無ければ上の段のタブ）。 */
export function featureFocusId(prefix: string, feature: JobFeature): string {
  return featureLabelId(prefix, feature);
}

export function JobFeatureTabs({ value, onChange, remembered, prefix, onLeaveHidden, children }: { value: JobFeature; onChange: (value: JobFeature) => void; remembered: Partial<Record<string, JobFeature>>; prefix: string; onLeaveHidden?: () => void; children: ReactNode }) {
  const group = groupOf(value) ?? jobFeatureGroups[0];
  if (!group) return null;
  const visibleGroups = jobFeatureGroups.filter(item => !item.hidden);
  const selectGroup = (id: string) => {
    const target = jobFeatureGroups.find(item => item.id === id);
    const next = remembered[id] ?? target?.features[0]?.id;
    if (next) onChange(next);
  };
  // 隠すグループを開いているときは、上の段のどのタブも選ばれていない。Tab キーで入れるよう先頭を 0 にする。
  const focusable = (id: string) => group.hidden ? id === visibleGroups[0]?.id : group.id === id;
  return <div className="jc-feature-workspace">
    <div className="jc-feature-primary" role="tablist" aria-label="求人管理の機能">{visibleGroups.map(item => <button type="button" role="tab" id={`${prefix}-group-${item.id}`} key={item.id} aria-selected={group.id === item.id} aria-controls={`${prefix}-group-panel`} tabIndex={focusable(item.id) ? 0 : -1} onClick={() => { selectGroup(item.id); }} onKeyDown={event => { keyboardSelection(event, visibleGroups.map(entry => entry.id), item.id, selectGroup, `${prefix}-group`); }}>{item.label}</button>)}</div>
    <section className="jc-feature-group-panel" role="tabpanel" id={`${prefix}-group-panel`} aria-labelledby={group.hidden ? `${prefix}-hidden-${group.id}` : `${prefix}-group-${group.id}`}>
      {group.hidden && <div className="jc-feature-hidden-heading"><h2 id={`${prefix}-hidden-${group.id}`} tabIndex={-1}>{group.label}</h2>{onLeaveHidden && <button type="button" className="jc-button" onClick={onLeaveHidden}>タイムラインに戻る</button>}</div>}
      <p className="jc-feature-purpose">{group.features.find(feature => feature.id === value)?.description}</p>
      {children}
    </section>
  </div>;
}

export function JobFeaturePanel({ feature, active, prefix, children }: { feature: JobFeature; active: boolean; prefix: string; children: ReactNode }) {
  return <div className="jc-feature-panel" role="tabpanel" id={`${prefix}-panel-${feature}`} aria-labelledby={featureLabelId(prefix, feature)} hidden={!active} tabIndex={0}>{children}</div>;
}
