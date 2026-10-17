import type { PropertyCondition } from './queuePropertyFilter';
import type { ValueChoiceSource } from './queueValueChoices';

/** パイプラインの肯定・否定条件をすべて満たす候補。一覧の選択範囲には依存しない。 */
export function metricGroupChoices(group: readonly PropertyCondition[], source: ValueChoiceSource): ValueChoiceSource {
  const pipelines = source.pipelines.filter(p => group.filter(c => c.property === 'pipeline').every(c => {
    if (c.operator === 'IN') return c.values.includes(p.id);
    if (c.operator === 'NOT_IN') return !c.values.includes(p.id);
    return c.operator !== 'NOT_HAS_PROPERTY';
  }));
  return { ...source, selectedPipelineIds: pipelines.map(p => p.id), allDeals: false };
}

export function metricGroupValid(group: readonly PropertyCondition[], source: ValueChoiceSource): boolean {
  const scope = metricGroupChoices(group, source);
  const stages = new Set(source.pipelines.filter(p => scope.selectedPipelineIds.includes(p.id)).flatMap(p => p.stages.map(s => s.id)));
  return group.filter(c => c.property === 'dealstage').every(c => c.values.every(v => stages.has(v)));
}
