import { useState } from 'react';
import { apiGet } from '../../api/client';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { MoveModal } from '../crm/StageMove';
import type { Target } from '../crm/StageMove';
import type { StageMoveCtx } from '../crm/writeBindings';
import type { CrmWrite } from '../crm/useCrmWrite';
import { fieldDefFromEditable, ruleFieldNames } from '../crm/writeModel';
import type { SaveRequest } from '../crm/writeModel';
import type { PropValues } from '../crm/writeTypes';

/** 現在値をすべて読めたときだけ共通の移動ダイアログを開く。読めない値を空欄とみなして送らない。 */
export function DealStageMove({ dealId, write, detail, props, request, locked, onStart, onDone }: {
  dealId: string; write: CrmWrite; detail: WorkspaceResponse; props: CrmCatalogProperty[];
  request: SaveRequest; locked: boolean; onStart: (names: string[]) => void; onDone: () => void;
}) {
  const [flow, setFlow] = useState<{ target: Target; ctx: StageMoveCtx } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const schema = write.schema.phase === 'ready' ? write.schema.schema : null;
  const overlay = write.stageOverlay[dealId];
  const stageId = overlay && (overlay.pending || !overlay.fetchedAt || detail.fetched_at <= overlay.fetchedAt) ? overlay.stage_id : detail.deal.stage_id;
  const label = overlay && (overlay.pending || !overlay.fetchedAt || detail.fetched_at <= overlay.fetchedAt) ? overlay.label : detail.deal.stage_label;
  // pipelines に別の区分が含まれても、現在の区分の stages の順だけを使う。
  const stages = schema?.pipeline_id === detail.deal.pipeline_id ? schema.stages : [];
  async function pick(id: string) {
    const targetStage = stages.find(stage => stage.id === id);
    if (!schema || !targetStage || id === stageId || !detail.deal.pipeline_id) return;
    setLoading(true); setError('');
    const target = { pipeline_id: detail.deal.pipeline_id, stage_id: id, label: targetStage.label };
    const defs = ruleFieldNames(targetStage).flatMap(name => {
      const prop = schema.editable.find(p => p.object === 'deal' && p.name === name);
      const def = prop ? fieldDefFromEditable(prop) : null;
      return def ? [def] : [];
    });
    const unavailable = targetStage.required.filter(name => !defs.some(d => d.name === name));
    if (unavailable.length) {
      setError(`移動に必要な項目をこの画面で入力できません。HubSpotで確認してください（${unavailable.map(name => props.find(p => p.name === name)?.label ?? '未取得の項目').join('、')}）。`);
      setLoading(false); return;
    }
    const current: PropValues = {};
    // 最新の元値を取得する。契約欄で変更した項目は、編集開始時の元値を優先する。
    for (let offset = 0; offset < defs.length; offset += 50) {
      const query = new URLSearchParams({ deal_props: defs.slice(offset, offset + 50).map(d => d.name).join(','), fresh: '1' });
      const res = await apiGet<WorkspaceResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?${query}`, { timeoutMs: 35_000 });
      if (!res.ok || res.data.partial.some(p => p.part === 'selected_properties') || defs.slice(offset, offset + 50).some(d => !(d.name in res.data.selected.deal))) {
        setError('移動に必要な現在の値を取得できませんでした。もう一度ステージを選んでください。'); setLoading(false); return;
      }
      Object.assign(current, res.data.selected.deal);
    }
    for (const change of request.changes) current[change.name] = change.base;
    const ctx: StageMoveCtx = {
      writesEnabled: true, pipelineId: target.pipeline_id, stageId, stages, pipelines: [],
      ruleOf: sid => schema.stages.find(s => s.id === sid),
      defOf: name => defs.find(d => d.name === name) ?? null,
      groupOf: () => null, labelOf: name => props.find(p => p.name === name)?.label ?? null,
      meOwnerId: null, status: undefined, schemaFailed: false,
      currentValues: () => Promise.resolve(current),
      move: async (to, changes) => {
        onStart(changes.map(c => c.name));
        return write.save({ ...request, changes, stage: to });
      },
      saveFields: changes => write.save({ ...request, changes }),
      retry: req => { void write.save(req); },
    };
    setFlow({ target, ctx }); setLoading(false);
  }
  return <div className="dc-stage">
    <label>ステージ：<select aria-label="ステージを変更" value={stageId ?? ''} disabled={locked || loading || flow !== null} onChange={e => { void pick(e.target.value); }}>
      {!stages.some(s => s.id === stageId) && <option value={stageId ?? ''}>{label ?? '現在のステージ'}</option>}
      {stages.map(s => <option key={s.id} value={s.id}>{s.label}</option>)}
    </select></label>
    {loading && <p role="status">移動に必要な項目を確認しています…</p>}
    {error && <p role="alert" className="dc-status dc-red">{error}</p>}
    {flow && <MoveModal ctx={flow.ctx} target={flow.target} extraChanges={request.changes} presets={Object.fromEntries(request.changes.map(c => [c.name, c.value ?? '']))} onClose={() => { setFlow(null); }} onDone={onDone} />}
  </div>;
}
