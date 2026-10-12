import type { QueueFilters } from './queueModel';
import { groupsOf } from './queueModel';
import { eligibleStageIds, findPipeline } from './queuePipelines';
import { parsePropertySort } from './queuePropertyFilter';
import type { PropertyCondition } from './queuePropertyFilter';
export function conditionsFit(f: QueueFilters, gs = groupsOf(f), today = new Intl.DateTimeFormat('sv-SE', { timeZone: 'Asia/Tokyo' }).format(new Date())): boolean {
  const stagesForLimit = f.stages.length ? f.stages : eligibleStageIds(f.pipeline);
  const selectedRules = (findPipeline(f.pipeline)?.stages ?? []).filter(s => stagesForLimit.includes(s.id));
  const always = selectedRules.some(s => s.rule === 'all');
  const whenDue = selectedRules.some(s => s.rule === 'due');
  const ownerCount = f.owner !== '' && f.owner !== 'all' ? 1 : 0;
  const lastCount = Number(Boolean(f.lastFrom)) + Number(Boolean(f.lastTo));
  const nextCount = Number(Boolean(f.nextFrom)) + Number(Boolean(f.nextTo));
  const dueEnd = f.nextTo && f.nextTo < today ? f.nextTo : today;
  const dueExists = !f.nextFrom || f.nextFrom <= dueEnd;
  const tomorrowDate = new Date(`${today}T00:00:00Z`); tomorrowDate.setUTCDate(tomorrowDate.getUTCDate() + 1);
  const tomorrow = tomorrowDate.toISOString().slice(0, 10);
  const tailStart = f.nextFrom && f.nextFrom > tomorrow ? f.nextFrom : tomorrow;
  const tailExists = !f.nextTo || tailStart <= f.nextTo;
  const sizes = (scope: 'due' | 'tail' | 'queue', last: boolean): number[] => {
    const common = ownerCount + (lastCount || Number(last));
    const dueSize = 2 + Number(Boolean(f.nextFrom)) + common;
    if (scope === 'due') return (always || whenDue) && dueExists ? [dueSize] : [];
    if (scope === 'tail') return !always || nextCount > 0 && !tailExists ? [] : nextCount ? [2 + Number(Boolean(f.nextTo)) + common] : [2 + common, 2 + common];
    return [...(always ? [1 + nextCount + common] : []), ...(whenDue && dueExists ? [dueSize] : [])];
  };
  const scopes = parsePropertySort(f.sort) !== null ? [sizes(f.due === 'today' ? 'due' : 'queue', false)]
    : f.sort.startsWith('last_call_') ? [...(lastCount ? [] : [sizes(f.due === 'today' ? 'due' : 'queue', true)]), sizes(f.due === 'today' ? 'due' : 'queue', true)]
    : [sizes('due', false), ...(f.due === 'today' ? [] : [...(lastCount ? [] : [sizes('tail', true)]), sizes('tail', true)])];
  if (gs.length > 3 || gs.some(g => g.length > 6) || scopes.some(bs => bs.length * gs.length > 5
    || bs.some(n => gs.some(g => n + g.length > 6)) || bs.reduce((sum, n) => sum + gs.reduce((m, g) => m + n + g.length, 0), 0) > 18)) {
    return false;
  }

  return true;
}
export function remainingConditions(f: QueueFilters, gs: PropertyCondition[][], at: number): number {
  let remaining = 0;
  for (let n = 1; n <= 6 - (gs[at]?.length ?? 0); n++) {
    const next = gs.map(g => [...g]);
    const placeholder: PropertyCondition = { property: 'sample', operator: 'NOT_HAS_PROPERTY', values: [] };
    next[at] ??= [];
    next[at].push(...Array.from({length:n}, () => placeholder));
    if (!conditionsFit(f, next)) break;
    remaining = n;
  }
  return remaining;
}
