import { HUBSPOT_ONLY_NOTE, NOT_CAUSAL_NOTE } from './format';

/**
 * 1 パネル 1 行の注意書きと、「ⓘ 集計の前提」の折りたたみ。
 * 折りたたみには、HubSpot に記録された応募だけを数えていることと、
 * 相関は因果ではないことの 2 つを必ず入れる（`includeHubSpot={false}` は応募を扱わないパネルだけ）。
 */
export function AssumptionsNote({ summary, items = [], includeHubSpot = true, className = '' }: { summary: string; items?: (string | null | undefined | false)[]; includeHubSpot?: boolean; className?: string }) {
  const notes = [...items.filter((item): item is string => typeof item === 'string' && item.trim() !== ''), ...(includeHubSpot ? [HUBSPOT_ONLY_NOTE] : []), NOT_CAUSAL_NOTE];
  return <div className={`jc-assumptions ${className}`.trim()}>
    <p className="jc-assumptions-line">{summary}</p>
    <details className="jc-assumptions-details"><summary>ⓘ 集計の前提</summary><ul>{notes.map(note => <li key={note}>{note}</li>)}</ul></details>
  </div>;
}
