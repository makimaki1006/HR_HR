import { groupsOf, selectedPipelineIds, selectedStageIds } from './queueModel';
import type { QueueFilters, QueueMode } from './queueModel';
import type { PropertyCondition } from './queuePropertyFilter';
import { findPipeline } from './queuePipelines';

/** 書き込みに成功した内容 (案件の項目と、ステージを移したとき) */
export interface WrittenDeal {
  /** 案件の項目 (内部名 → 書いた値。空にしたときは null) */
  props: Readonly<Record<string, string | null>>;
  stage?: { pipeline_id: string; stage_id: string } | undefined;
}

export interface DropContext {
  filters: QueueFilters;
  mode: QueueMode;
  /** JST の今日 (YYYY-MM-DD) */
  today: string;
  /** ログイン中の人の HubSpot owner ID (分からなければ null) */
  meOwnerId: string | null;
  /** 項目の型 (分からなければ null) */
  kindOf: (property: string) => string | null;
  /** 一覧の行が持つ今のステージ (書き込みでステージを移さなかったときの判定用) */
  currentStageId: string | null;
}

/** true = 条件に合う / false = 合わない / null = この画面の中では判定できない */
type Tri = boolean | null;
type Known = Readonly<Record<string, string | null>>;

const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;
const BUILT_IN_KINDS: Readonly<Record<string, string>> = { bpo_13: 'date', bpo_20: 'date' };

function shiftDate(today: string, days: number): string {
  const d = new Date(`${today}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

function and(parts: readonly Tri[]): Tri {
  if (parts.some(p => p === false)) return false;
  return parts.every(p => p === true) ? true : null;
}
function or(parts: readonly Tri[]): Tri {
  if (parts.some(p => p === true)) return true;
  return parts.every(p => p === false) ? false : null;
}

/** 日付の項目の値を YYYY-MM-DD にする。その形でなければ判定しない (エポック ms など) */
function isoDate(value: string): string | null {
  if (ISO_DATE.test(value)) return value;
  return /^\d{4}-\d{2}-\d{2}T/.test(value) ? value.slice(0, 10) : null;
}

function evalCondition(c: PropertyCondition, known: Known, today: string, kindOf: (p: string) => string | null): Tri {
  if (!(c.property in known)) return null;
  const value = known[c.property] ?? null;
  const present = value !== null && value !== '';
  const kind = kindOf(c.property) ?? BUILT_IN_KINDS[c.property] ?? null;
  const first = c.values[0] ?? '';
  switch (c.operator) {
    case 'HAS_PROPERTY': return present;
    case 'NOT_HAS_PROPERTY': return !present;
    case 'IS_TRUE': return value === 'true';
    case 'IS_FALSE': return value === 'false';
    case 'IN': return present ? c.values.includes(value) : false;
    // 空の値が「合う」側か「合わない」側かは HubSpot の扱いに頼るので、空のときは判定しない
    case 'NOT_IN': return present ? !c.values.includes(value) : null;
    case 'EQ': case 'NEQ': {
      if (!present) return c.operator === 'EQ' ? false : null;
      if (kind === 'number') {
        const a = Number(value); const b = Number(first);
        if (!Number.isFinite(a) || !Number.isFinite(b)) return null;
        return c.operator === 'EQ' ? a === b : a !== b;
      }
      if (kind === 'string' || kind === 'enumeration') {
        const same = value.toLowerCase() === first.toLowerCase();
        return c.operator === 'EQ' ? same : !same;
      }
      return null;
    }
    case 'GTE': case 'LTE': case 'BETWEEN': {
      if (!present) return false;
      if (kind === 'number') {
        const a = Number(value); const lo = Number(first); const hi = Number(c.values[1] ?? '');
        if (!Number.isFinite(a) || !Number.isFinite(lo)) return null;
        if (c.operator === 'GTE') return a >= lo;
        if (c.operator === 'LTE') return a <= lo;
        return Number.isFinite(hi) ? a >= lo && a <= hi : null;
      }
      if (kind === 'date') {
        const a = isoDate(value);
        const hi = c.values[1] ?? '';
        if (a === null || !ISO_DATE.test(first) || (c.operator === 'BETWEEN' && !ISO_DATE.test(hi))) return null;
        if (c.operator === 'GTE') return a >= first;
        if (c.operator === 'LTE') return a <= first;
        return a >= first && a <= hi;
      }
      return null;
    }
    case 'TODAY': case 'BEFORE_TODAY': case 'THIS_MONTH': case 'LAST_N_DAYS': case 'NEXT_N_DAYS': {
      if (!present) return false;
      // 日時の項目は UTC / JST の日付境界がずれるので判定しない
      if (kind !== 'date') return null;
      const a = isoDate(value);
      if (a === null) return null;
      if (c.operator === 'TODAY') return a === today;
      if (c.operator === 'BEFORE_TODAY') return a < today;
      if (c.operator === 'THIS_MONTH') return a.slice(0, 7) === today.slice(0, 7);
      const n = Number(first);
      if (!Number.isInteger(n) || n < 1) return null;
      return c.operator === 'LAST_N_DAYS' ? a >= shiftDate(today, -n + 1) && a <= today : a >= today && a <= shiftDate(today, n - 1);
    }
    default: return null; // 文字を含む / 含まない など
  }
}

function dateRange(known: Known, property: string, from: string, to: string): Tri {
  if (!from && !to) return true;
  if (!(property in known)) return null;
  const v = known[property] ?? null;
  if (v === null || v === '') return false;
  const d = isoDate(v);
  if (d === null) return null;
  return (!from || d >= from) && (!to || d <= to);
}

/** 次回日が来たものだけ (今日以前) の判定 */
function dueByToday(value: string | null, today: string): Tri {
  if (value === null || value === '') return false;
  const d = isoDate(value);
  return d === null ? null : d <= today;
}

/**
 * 書き込みに成功した案件が、いまの一覧の条件から外れたか。書いた値と条件を比べ、**外れたと言い切れるときだけ** true。
 * 書いていない項目に関わる条件・型が分からない条件・判定の仕方に曖昧さがある条件は、外さずに残す (false)。
 */
export function leavesQueue(w: WrittenDeal, ctx: DropContext): boolean {
  const f = ctx.filters;
  const known: Record<string, string | null> = { ...w.props };
  if (w.stage) { known.dealstage = w.stage.stage_id; known.pipeline = w.stage.pipeline_id; }
  const parts: Tri[] = [];
  const pipelines = selectedPipelineIds(f);

  // パイプライン・ステージ
  if (w.stage) {
    if (!pipelines.includes(w.stage.pipeline_id)) parts.push(false);
    else if (f.stages.length > 0) parts.push(f.stages.includes(w.stage.stage_id));
    else if (f.target !== 'all') parts.push(selectedStageIds(f).includes(w.stage.stage_id));
  }

  // 架電対象 (「すべての取引」でないとき) の決まり: 架電禁止・ブロックの理由が入ったら出さない。次回日が来たものだけのステージ
  if (f.target !== 'all') {
    for (const stopProp of ['bpo_3', 'bpo_4']) {
      if (stopProp in w.props && (w.props[stopProp] ?? '') !== '') parts.push(false);
    }
    const stageId = w.stage?.stage_id ?? ctx.currentStageId;
    const pipeline = w.stage ? findPipeline(w.stage.pipeline_id) : pipelines.map(id => findPipeline(id)).find(p => p?.stages.some(s => s.id === stageId));
    const rule = pipeline?.stages.find(s => s.id === stageId)?.rule;
    if ((rule === 'due' || f.due === 'today') && 'bpo_13' in w.props) parts.push(dueByToday(w.props.bpo_13 ?? null, ctx.today));
  }

  // 担当者
  if ('hubspot_owner_id' in w.props) {
    const owner = w.props.hubspot_owner_id ?? null;
    if (f.owner === 'unassigned') parts.push(owner === null || owner === '');
    else if (f.owner === 'me') { if (ctx.meOwnerId !== null) parts.push(owner === ctx.meOwnerId); }
    else if (f.owner !== '' && f.owner !== 'all') parts.push(owner === f.owner);
  }

  // 日付の範囲 (次回架電日 bpo_13 / 最終架電日 bpo_20)
  parts.push(dateRange(known, 'bpo_13', f.nextFrom, f.nextTo), dateRange(known, 'bpo_20', f.lastFrom, f.lastTo));

  // 詳細条件: まとまりの中は AND、まとまり同士は OR
  const groups = groupsOf(f);
  if (groups.length > 0) parts.push(or(groups.map(g => and(g.map(c => evalCondition(c, known, ctx.today, ctx.kindOf))))));
  return parts.some(p => p === false);
}
