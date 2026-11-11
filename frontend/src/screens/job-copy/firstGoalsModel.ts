import type { PropValues } from '../crm/writeTypes';
import type { SaveRequest } from '../crm/writeModel';

/**
 * 初回MTGで顧客と決める 2 つの目標。保存先は HubSpot の取引の 3 項目。
 * - マインド目標 / コミット目標: 人が読む文 (1 行 1 目標)。HubSpot の画面でもそのまま読める。
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
export const KIND_LABEL: Record<GoalKind, string> = { ideal: 'マインド目標', minimum: 'コミット目標', unclassified: '区分不明' };
export const KIND_NOTE: Record<GoalKind, string> = {
  ideal: '（目指す難易度が高い目標）',
  minimum: '（絶対に達成する目標）',
  unclassified: 'MTGからはどちらか決められませんでした。マインド目標かコミット目標を選んでください。',
};

export interface Goal { kind: GoalKind; what: string; target: string; by_when: string; notes?: string[] }
export interface GoalMeeting { start_jst?: string; topic?: string; sources?: string[]; notes?: string[] }
export interface GoalData {
  v: 1;
  /** draft = MTG から読み取った案 (未確認)、confirmed = 担当者が確かめて保存した */
  status: 'draft' | 'confirmed';
  goals: Goal[];
  notes?: string[];
  meeting?: GoalMeeting;
  updated_at?: string;
}

const SOURCE_LABEL: Record<string, string> = {
  transcript: 'Zoom の文字起こし', asr: '録音の音声認識', summary: 'Zoom AI Companion の要約', slide: '画面に映した目標の資料',
};
export const sourceLabel = (source: string) => SOURCE_LABEL[source] ?? source;

export function goalNotes(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((note): note is string => typeof note === 'string' && note.trim() !== '') : [];
}

/** 詳細表示用。HubSpot の本文が訂正されていても、読み取り時の注記を表示する。 */
export function goalDisplayNotes(values: Record<string, string | null | undefined>): string[] {
  try {
    const data = JSON.parse(values[GOAL_PROPS.data] ?? '') as { notes?: unknown; goals?: ({ notes?: unknown } | null)[] } | null;
    return [...new Set([...goalNotes(data?.notes), ...(Array.isArray(data?.goals) ? data.goals.flatMap(goal => goalNotes(goal?.notes)) : [])])];
  } catch { return []; }
}

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

/** データ (JSON) だけから目標を読む。読めなければ null。 */
function parseData(rawText: string | null | undefined): GoalData | null {
  try {
    const raw = JSON.parse(rawText ?? '') as Partial<GoalData> | null;
    if (!raw || !Array.isArray(raw.goals)) return null;
    return {
      v: 1,
      status: raw.status === 'draft' ? 'draft' : 'confirmed',
      goals: raw.goals.flatMap(item => {
        const goal = item as Partial<Goal> | null;
        return goal && text(goal.what).trim() ? [{ kind: isKind(goal.kind) ? goal.kind : 'unclassified', what: text(goal.what), target: text(goal.target), by_when: text(goal.by_when), ...(goalNotes(goal.notes).length ? { notes: goalNotes(goal.notes) } : {}) }] : [];
      }),
      ...(goalNotes(raw.notes).length ? { notes: goalNotes(raw.notes) } : {}),
      ...(raw.meeting ? { meeting: raw.meeting } : {}),
      ...(raw.updated_at ? { updated_at: raw.updated_at } : {}),
    };
  } catch { return null; }
}

/**
 * HubSpot で文だけが直され、データ (JSON) が古いままか。画面は文を正として読み、確定済みとして扱う
 * (夜のバッチも、文が直された取引には書かない)。次に保存すると 3 項目がそろう。
 */
export function textEditedInHubSpot(values: PropValues): boolean {
  const parsed = parseData(values[GOAL_PROPS.data]);
  return parsed !== null && (goalsText(parsed.goals, 'ideal') !== (values[GOAL_PROPS.ideal] ?? '').trim() || goalsText(parsed.goals, 'minimum') !== (values[GOAL_PROPS.minimum] ?? '').trim());
}

/** HubSpot の 3 項目から目標を読む。何も入っていなければ null。 */
export function readGoals(values: PropValues): GoalData | null {
  const idealText = values[GOAL_PROPS.ideal] ?? '';
  const minimumText = values[GOAL_PROPS.minimum] ?? '';
  const parsed = parseData(values[GOAL_PROPS.data]);
  const handEdited = parsed !== null && (goalsText(parsed.goals, 'ideal') !== idealText.trim() || goalsText(parsed.goals, 'minimum') !== minimumText.trim());
  if (parsed && !handEdited) return parsed.goals.length > 0 || idealText.trim() || minimumText.trim() ? parsed : null;
  // HubSpot の画面で文を直した (または文だけ入っている): 文を正として読み直し、区分不明は残す。
  // 文を消していれば、データに古い目標が残っていても出さない。
  const unclassified = parsed?.goals.filter(goal => goal.kind === 'unclassified') ?? [];
  const goals = [...fromLines(idealText, 'ideal'), ...fromLines(minimumText, 'minimum'), ...unclassified];
  if (goals.length === 0) return null;
  return {
    v: 1,
    status: 'confirmed',
    goals,
    ...(parsed && goalDisplayNotes(values).length ? { notes: goalDisplayNotes(values) } : {}),
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
  if (goals.some(goal => goal.kind === 'unclassified')) problems.push('区分不明の目標があります。マインド目標かコミット目標を選んでください。');
  return problems;
}
export function goalWarnings(goals: Goal[]): string[] {
  const warnings: string[] = [];
  if (!goals.some(goal => goal.kind === 'ideal')) warnings.push('マインド目標がありません。');
  if (!goals.some(goal => goal.kind === 'minimum')) warnings.push('コミット目標がありません。');
  return warnings;
}

/** 確定として保存する。いつまでが空なら「契約期間中」を入れる。 */
export function confirmedData(goals: Goal[], previous: GoalData | null, now: Date): GoalData {
  return {
    v: 1,
    status: 'confirmed',
    goals: goals.map(goal => ({ kind: goal.kind, what: goal.what.trim(), target: goal.target.trim(), by_when: goal.by_when.trim() || DEFAULT_BY_WHEN, ...(goal.notes ? { notes: goal.notes } : {}) })),
    ...(previous?.notes ? { notes: previous.notes } : {}),
    ...(previous?.meeting ? { meeting: previous.meeting } : {}),
    updated_at: now.toISOString(),
  };
}

/** 案を直さずにそのまま確定する。 */
export function confirmDraft(data: GoalData, now: Date): GoalData {
  return confirmedData(data.goals, data, now);
}

const withoutTime = (data: GoalData) => JSON.stringify({ ...data, updated_at: undefined });

/**
 * 3 項目を 1 回で送る。変わらない項目も、編集を始めたときの値を base にして必ず送る
 * (サーバは送った項目だけを HubSpot の今の値と比べる。1 項目でも誰かが直していれば上書きしない)。
 * 文・データ・確定済みであることがすべて同じなら何も送らない。
 */
export function goalsRequest(dealId: string, base: PropValues, data: GoalData): SaveRequest {
  const next: Record<string, string | null> = {
    [GOAL_PROPS.ideal]: goalsText(data.goals, 'ideal') || null,
    [GOAL_PROPS.minimum]: goalsText(data.goals, 'minimum') || null,
    // 目標を全部消したときもデータは残す (確定済みの印。夜のバッチが新しい案を書かないように)
    [GOAL_PROPS.data]: JSON.stringify(data),
  };
  const prev = (() => {
    try { return JSON.parse(base[GOAL_PROPS.data] ?? '') as GoalData | null; } catch { return null; }
  })();
  const same = (base[GOAL_PROPS.ideal] ?? null) === next[GOAL_PROPS.ideal] && (base[GOAL_PROPS.minimum] ?? null) === next[GOAL_PROPS.minimum]
    && prev !== null && prev.status === 'confirmed' && withoutTime(prev) === withoutTime(data);
  return {
    dealId,
    changes: same ? [] : GOAL_PROP_NAMES.map(name => ({ object: 'deal' as const, name, base: base[name] ?? null, value: next[name] ?? null })),
  };
}
