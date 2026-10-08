import { useState } from 'react';
import { eligibleStageIds, stageName } from './queuePipelines';
import type { QueuePipelineDef } from './queuePipelines';
import { normalizeStages } from './queueModel';

const RULE_NOTES: Record<string, string> = { all: '常に表示', due: '次回架電日が来たら表示' };

/**
 * ステージの絞り込み (選んだパイプラインの対象ステージのチェックリスト)。
 * ボタン「ステージ(n件選択)」で開き、「適用」で反映する (チェックのたびに HubSpot を呼ばない)。
 * 既定はすべて選択。架電対象外のステージは下に灰色で並べるだけ (選べない)。
 */
export function StageFilter({ pipeline, selected, labelsUnavailable = false, onApply }: {
  pipeline: QueuePipelineDef;
  /** 条件の値 (空 = すべて) */
  selected: readonly string[];
  /** HubSpot からステージ名を読めなかった (ID で出している) */
  labelsUnavailable?: boolean;
  onApply: (stages: string[]) => void;
}) {
  const eligible = eligibleStageIds(pipeline.id);
  const current = selected.length > 0 ? selected : eligible;
  const [draft, setDraft] = useState<Set<string> | null>(null);
  const open = draft !== null;
  const excluded = [...pipeline.stages.filter(s => s.rule === 'exclude'), ...pipeline.unknownStages];
  const unknownIds = new Set(pipeline.unknownStages.map(s => s.id));
  const panelId = `cq-stage-list-${pipeline.id}`;

  function toggle(id: string) {
    setDraft(prev => {
      const next = new Set(prev ?? []);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }
  function apply() {
    if (!draft || draft.size === 0) return;
    onApply(normalizeStages(pipeline.id, draft));
    setDraft(null);
  }

  return <div className="cq-stage-filter" onKeyDown={e => { if (e.key === 'Escape' && open) { e.stopPropagation(); setDraft(null); } }}>
    <button type="button" className="cq-btn cq-stage-toggle" aria-expanded={open} aria-controls={panelId}
      onClick={() => { setDraft(open ? null : new Set(current)); }}>
      ステージ（{current.length}件選択）</button>
    {open && <div id={panelId} className="cq-stage-panel" role="group" aria-label="ステージの選択">
      <div className="cq-stage-actions">
        <button type="button" className="cq-btn cq-btn-quiet" onClick={() => { setDraft(new Set(eligible)); }}>すべて選択</button>
        <button type="button" className="cq-btn cq-btn-quiet" onClick={() => { setDraft(new Set()); }}>すべて外す</button>
      </div>
      {labelsUnavailable && <p className="cq-stage-warn">HubSpot からステージ名を読めなかったため、ID で表示しています</p>}
      <div className="cq-stage-scroll">
        <ul className="cq-stage-list">
          {pipeline.stages.filter(s => s.rule !== 'exclude').map(s => <li key={s.id}>
            <label><input type="checkbox" checked={draft.has(s.id)} onChange={() => { toggle(s.id); }} />
              <span>{stageName(s)}</span><small>{RULE_NOTES[s.rule]}</small></label>
          </li>)}
        </ul>
        {excluded.length > 0 && <div className="cq-stage-excluded" aria-label="架電対象外のステージ">
          <p>架電対象外</p>
          <ul>{excluded.map(s => <li key={s.id} aria-disabled="true">{stageName(s)}{unknownIds.has(s.id) && <small>(架電キューの設定に無いステージ)</small>}</li>)}</ul>
        </div>}
      </div>
      {draft.size === 0 && <p className="cq-stage-warn" role="status">ステージを 1 つ以上選んでください</p>}
      <div className="cq-stage-actions">
        <button type="button" className="cq-btn" onClick={apply} disabled={draft.size === 0}>適用</button>
        <button type="button" className="cq-btn cq-btn-quiet" onClick={() => { setDraft(null); }}>キャンセル</button>
      </div>
    </div>}
  </div>;
}
