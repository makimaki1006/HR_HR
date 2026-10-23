import type { PropValues } from '../crm/writeTypes';
import type { SaveRequest } from '../crm/writeModel';

/**
 * 初回MTGで顧客と決める 2 つの目標。保存先は HubSpot の取引の 3 項目。
 * - 理想 / 前進ライン: 人が読む文 (1 行 1 目標)。HubSpot の画面でもそのまま読める。
 * - データ: 画面と夜のバッチが使う JSON (案か確定か、MTG の根拠など)。
 * HubSpot で文を直接直した場合は、文のほうを正とする (JSON から作った文と食い違えば文を読み直す)。
 */
export const GOAL_PROPS = {
  ideal: 'shokai_mtg_mokuhyou_risou',
  minimum: 'shokai_mtg_mokuhyou_zenshin',
  data: 'shokai_mtg_mokuhyou_data',
} as const;
export const GOAL_PROP_NAMES = [GOAL_PROPS.ideal, GOAL_PROPS.minimum, GOAL_PROPS.data] as const;
export const DEFAULT_BY_WHEN = '契約期間中';

export type GoalKind = 'ideal' | 'minimum' | 'unclassified';
export const KIND_LABEL: Record<GoalKind, string> = { ideal: '理想', minimum: '前進ライン', unclassified: '区分不明' };
export const KIND_NOTE: Record<GoalKind, string> = {
  ideal: '頑張ればできそうな目標（コミット目標）',
  minimum: '必ず達成する目標（マインド目標）',
  unclassified: 'MTGからはどちらか決められませんでした。理想か前進ラインを選んでください。',
};

export interface Goal { kind: GoalKind; what: string; target: string; by_when: string }
export interface GoalMeeting { start_jst?: string; topic?: string; sources?: string[]; notes?: string[] }
export interface GoalData {
  v: 1;
  /** draft = MTG から読み取った案 (未確認)、confirmed = 担当者が確かめて保存した */
  status: 'draft' | 'confirmed';
  goals: Goal[];
  meeting?: GoalMeeting;
  updated_at?: string;
}

const SOURCE_LABEL: Record<string, string> = {
  transcript: 'Zoom の文字起こし', asr: '録音の音声認識', summary: 'Zoom AI Companion の要約', slide: '画面に映した目標の資料',
};
export const sourceLabel = (source: string) => SOURCE_LABEL[source] ?? source;

export function goalLine(goal: Goal): string {
  const target = goal.target.trim();
  return `${goal.what.trim()}${target ? `（${target}）` : ''}／${goal.by_when.trim() || DEFAULT_BY_WHEN}`;
}
export function goalsText(goals: Goal[], kind: 'ideal' | 'minimum'): string {
  return goals.filter(goal => goal.kind === kind).map(goalLine).join('\n');
}

function isKind(value: unknown): value is GoalKind {
  return value === 'ideal' || value === 'minimum' || value === 'unclassified';
}
function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}
function fromLines(raw: string | null | undefined, kind: 'ideal' | 'minimum'): Goal[] {
  return (raw ?? '').split('\n').map(line => line.trim()).filter(Boolean).map(line => {
    const match = /^(.*?)(?:（(.*)）)?／(.*)$/.exec(line);
    return match ? { kind, what: match[1] ?? line, target: match[2] ?? '', by_when: match[3] ?? '' } : { kind, what: line, target: '', by_when: '' };
  });
}

/** HubSpot の 3 項目から目標を読む。何も入っていなければ null。 */
export function readGoals(values: PropValues): GoalData | null {
  const idealText = values[GOAL_PROPS.ideal] ?? '';
  const minimumText = values[GOAL_PROPS.minimum] ?? '';
  let parsed: GoalData | null = null;
  try {
    const raw = JSON.parse(values[GOAL_PROPS.data] ?? '') as Partial<GoalData> | null;
    if (raw && Array.isArray(raw.goals)) {
      parsed = {
        v: 1,
        status: raw.status === 'draft' ? 'draft' : 'confirmed',
        goals: raw.goals.flatMap(item => {
          const goal = item as Partial<Goal> | null;
          return goal && text(goal.what).trim() ? [{ kind: isKind(goal.kind) ? goal.kind : 'unclassified', what: text(goal.what), target: text(goal.target), by_when: text(goal.by_when) }] : [];
        }),
        ...(raw.meeting ? { meeting: raw.meeting } : {}),
        ...(raw.updated_at ? { updated_at: raw.updated_at } : {}),
      };
    }
  } catch { parsed = null; }
  const handEdited = parsed !== null && (goalsText(parsed.goals, 'ideal') !== idealText.trim() || goalsText(parsed.goals, 'minimum') !== minimumText.trim());
  if (parsed && !handEdited) return parsed;
  if (!idealText.trim() && !minimumText.trim()) return parsed && parsed.goals.length > 0 ? parsed : null;
  // HubSpot の画面で文を直した (または文だけ入っている): 文を正として読み直し、区分不明は残す。
  const unclassified = parsed?.goals.filter(goal => goal.kind === 'unclassified') ?? [];
  return {
    v: 1,
    status: 'confirmed',
    goals: [...fromLines(idealText, 'ideal'), ...fromLines(minimumText, 'minimum'), ...unclassified],
    ...(parsed?.meeting ? { meeting: parsed.meeting } : {}),
  };
}

export function emptyGoal(kind: GoalKind): Goal {
  return { kind, what: '', target: '', by_when: '' };
}

/** 保存できない理由。空なら保存できる。 */
export function goalProblems(goals: Goal[]): string[] {
  const problems: string[] = [];
  if (goals.some(goal => !goal.what.trim())) problems.push('「何をするか」が空の目標があります。書くか、行を消してください。');
  if (goals.some(goal => goal.kind === 'unclassified')) problems.push('区分不明の目標があります。理想か前進ラインを選んでください。');
  return problems;
}
export function goalWarnings(goals: Goal[]): string[] {
  const warnings: string[] = [];
  if (!goals.some(goal => goal.kind === 'ideal')) warnings.push('理想の目標がありません。');
  if (!goals.some(goal => goal.kind === 'minimum')) warnings.push('前進ラインの目標がありません。');
  return warnings;
}

/** 確定として保存する。いつまでが空なら「契約期間中」を入れる。 */
export function confirmedData(goals: Goal[], previous: GoalData | null, now: Date): GoalData {
  return {
    v: 1,
    status: 'confirmed',
    goals: goals.map(goal => ({ kind: goal.kind, what: goal.what.trim(), target: goal.target.trim(), by_when: goal.by_when.trim() || DEFAULT_BY_WHEN })),
    ...(previous?.meeting ? { meeting: previous.meeting } : {}),
    updated_at: now.toISOString(),
  };
}

export function goalsRequest(dealId: string, base: PropValues, data: GoalData): SaveRequest {
  const next: Record<string, string | null> = {
    [GOAL_PROPS.ideal]: goalsText(data.goals, 'ideal') || null,
    [GOAL_PROPS.minimum]: goalsText(data.goals, 'minimum') || null,
    [GOAL_PROPS.data]: data.goals.length ? JSON.stringify(data) : null,
  };
  return {
    dealId,
    changes: GOAL_PROP_NAMES.filter(name => (base[name] ?? null) !== next[name]).map(name => ({ object: 'deal', name, base: base[name] ?? null, value: next[name] ?? null })),
  };
}
