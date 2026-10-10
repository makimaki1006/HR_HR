// 求人票生成パイプライン (/app/jobgen) の状態と純粋な補助関数。
// 旧 static/jobgen.html の `S` / `STEPS` / `STEP_INPUTS` / `staleAfter` / `stView` を移植。
// DOM もネットワークも触らない (Vitest は node 環境でこのファイルを直接検証する)。
import type { Analysis } from '../../generated/Analysis';
import type { FactField } from '../../generated/FactField';
import type { FillStats } from '../../generated/FillStats';
import type { GeneratedField } from '../../generated/GeneratedField';
import type { ImageDirection } from '../../generated/ImageDirection';
import type { ImagePrompt } from '../../generated/ImagePrompt';
import type { NgViolation } from '../../generated/NgViolation';
import type { NormalizedJob } from '../../generated/NormalizedJob';
import type { NumberViolation } from '../../generated/NumberViolation';
import type { Persona } from '../../generated/Persona';
import type { UnassignedHint } from '../../generated/UnassignedHint';

export type InputKind = 'free_text' | 'url' | 'csv' | 'excel' | 'pdf' | 'html';

export type StepKey =
  | 'extract'
  | 'analyze'
  | 'personas'
  | 'copy'
  | 'images'
  | 'mobile'
  | 'hrhacker'
  | 'ab';

/** 工程の状態。`stale` = 前工程が更新されたので要再実行。 */
export type StepStatus = 'wait' | 'run' | 'done' | 'fail' | 'review' | 'stale';

export interface StepDef {
  key: StepKey;
  num: string;
  name: string;
  /** コードによる検証ゲート。"—" は検証ゲートを持たない工程 (②③⑤⑧)。 */
  gate: string;
}

/** 工程定義 (設計正本 §3 の①〜⑧の日本語名)。旧画面と同じ順・同じ文言。 */
export const STEPS: readonly StepDef[] = [
  { key: 'extract', num: '①', name: '事実抽出', gate: '引用照合' },
  { key: 'analyze', num: '②', name: '市場分析', gate: '—' },
  { key: 'personas', num: '③', name: 'ペルソナ設計', gate: '—' },
  { key: 'copy', num: '④', name: 'キャッチコピー', gate: 'NGワード' },
  { key: 'images', num: '⑤', name: '画像案・生成プロンプト', gate: '—' },
  { key: 'mobile', num: '⑥', name: 'スマホ原稿', gate: '文字数・NGワード' },
  { key: 'hrhacker', num: '⑦', name: '84列原稿＋数値照合', gate: '数値照合・文字数・NGワード' },
  { key: 'ab', num: '⑧', name: 'A/Bテスト助言', gate: '—' },
];

export function stepDef(key: StepKey): StepDef {
  const def = STEPS.find((s) => s.key === key);
  if (!def) throw new Error(`unknown step: ${key}`);
  return def;
}

/** 各工程が入力に使う前工程 (直接の依存元)。単独再実行時に「古くなった」後続工程を辿る。 */
export const STEP_INPUTS: Record<StepKey, readonly StepKey[]> = {
  extract: [],
  analyze: [],
  personas: ['analyze'],
  copy: ['analyze', 'personas'],
  images: ['personas'],
  mobile: ['extract', 'personas'],
  hrhacker: ['extract', 'analyze'],
  ab: ['analyze', 'personas'],
};

/** `key` を実行し直したときに結果が古くなる後続工程 (推移的)。 */
export function staleAfter(key: StepKey): StepKey[] {
  const out = new Set<StepKey>();
  let changed = true;
  while (changed) {
    changed = false;
    for (const k of Object.keys(STEP_INPUTS) as StepKey[]) {
      if (out.has(k) || k === key) continue;
      if (STEP_INPUTS[k].some((i) => i === key || out.has(i))) {
        out.add(k);
        changed = true;
      }
    }
  }
  return [...out];
}

export const ST_LABEL: Record<StepStatus, string> = {
  wait: '待機',
  run: '実行中',
  done: '完了',
  fail: '失敗',
  review: 'レビュー要',
  stale: '要再実行（前工程が更新済み）',
};

/** 検証ゲートの有無で「完了」の意味を分ける。 */
export function isGated(key: StepKey): boolean {
  return stepDef(key).gate !== '—';
}

export interface StepView {
  cls: string;
  label: string;
}

/** 工程ピルの表示ラベル/クラス。コンサル確認済みが最優先で「品質確認済み」を示す。 */
export function stepView(key: StepKey, status: StepStatus, confirmed: boolean): StepView {
  if (confirmed && (status === 'done' || status === 'review')) {
    return { cls: 'confirmed', label: 'コンサル確認済み' };
  }
  if (status === 'done') {
    return {
      cls: 'done',
      label: isGated(key) ? '生成完了・自動検証済み' : '生成完了（自動検証なし）',
    };
  }
  if (status === 'review') return { cls: 'review', label: '警告あり（要確認）' };
  if (status === 'stale') return { cls: 'stale', label: ST_LABEL.stale };
  return { cls: status, label: ST_LABEL[status] };
}

/** 工程①の項目キー → 日本語。 */
export const FKEY_JA: Record<string, string> = {
  salary: '給与',
  working_hours: '勤務時間',
  holidays: '休日',
  work_location: '勤務地',
  employment_type: '雇用形態',
  insurance: '保険',
  allowances: '手当',
  required_qualifications: '必須資格',
};

/** 作成例プレビュー (⑥) の募集要項に出す項目と順。 */
export const PV_FACT_KEYS: readonly string[] = [
  'salary',
  'working_hours',
  'holidays',
  'work_location',
  'employment_type',
  'insurance',
  'allowances',
  'required_qualifications',
];

/** 工程①の facts (キーは FACT_KEYS の 8 つ)。noUncheckedIndexedAccess で添字は undefined を含む。 */
export type Facts = Record<string, FactField>;

/** ④ 1 ペルソナ分の結果 (旧 `S.copies` の要素)。 */
export interface CopyResult {
  label: string;
  copies: { style: string; text: string }[];
  ng_violations: NgViolation[];
  expression_warnings: NgViolation[];
  number_violations: NumberViolation[];
  number_check: string;
  review_required: boolean;
  error?: string;
}

/** ⑥ 1 ペルソナ分の結果 (旧 `S.mobile` の要素)。 */
export interface MobileResult {
  label: string;
  lines: string[];
  ng_violations: NgViolation[];
  expression_warnings: NgViolation[];
  number_violations: NumberViolation[];
  number_check: string;
  review_required: boolean;
  error?: string;
}

/** ⑦ の結果 (旧 `S.hrhacker`)。`row` のキー順が 84 列の列順。 */
export interface HrhackerResult {
  row: Record<string, string>;
  generated_fields: Partial<Record<string, GeneratedField>>;
  review_required_fields: string[];
  unsupported_numbers: string[];
  fill_stats: FillStats | null;
  unassigned_hints: UnassignedHint[];
}

/** ⑧ の結果 (旧 `S.ab`)。 */
export interface AbResult {
  steps: { metric: string; action: string }[];
  ng_violations: NgViolation[];
  expression_warnings: NgViolation[];
  number_violations: NumberViolation[];
  number_check: string;
}

export interface StatusMessage {
  kind: 'loading' | 'err';
  text: string;
}

export interface PipelineState {
  kind: InputKind;
  sourceKind: InputKind;
  titleHint: string;
  sourceText: string;
  jobs: NormalizedJob[];
  selectedJobIndex: number | null;
  /** 職種名欄 (②市場分析で使用)。取り込み時に title_hint で初期化。 */
  jobTitle: string;
  /** 職種名で市場分析してよいとコンサルが確認したか (一括実行ゲート)。 */
  jobTitleConfirmed: boolean;
  facts: Facts | null;
  factsText: string;
  category: string;
  analysis: Analysis | null;
  knowledgeUsed: boolean;
  personas: Persona[];
  copies: CopyResult[];
  images: ImageDirection[];
  imagePrompts: ImagePrompt[];
  imagePromptsError: string;
  /** 原文にない数値 (images + image_prompts を統合)。 */
  imagesNv: NumberViolation[];
  imagesNumberCheck: string;
  mobile: MobileResult[];
  hrhacker: HrhackerResult | null;
  hrhackerCreatedAt: string | null;
  ab: AbResult | null;
  status: Record<StepKey, StepStatus>;
  /** コンサルが目視確認済み (ページ内 state のみ・永続化なし)。 */
  confirmed: Record<StepKey, boolean>;
  /** 後続工程を古くした元の工程 (結果セクション上部の帯に使う)。 */
  staleSource: Partial<Record<StepKey, StepKey>>;
  /** 工程が失敗したときのメッセージ (結果セクションはこれだけを出す)。 */
  failures: Partial<Record<StepKey, string>>;
  /** 工程の結果を 1 度でも受け取ったか (結果セクションを出す条件。旧: el.hidden=false)。 */
  resultReady: Record<StepKey, boolean>;
  running: boolean;
  curStep: StepKey | null;
  /** 取り込み (normalize) の通信中 (取り込みボタンを無効化)。 */
  normalizing: boolean;
  statusMessage: StatusMessage | null;
  ctlHint: string;
  personaCount: number;
}

export const CTL_HINT_INITIAL = '先に求人原文を取り込むと実行できます。';
export const CTL_HINT_READY =
  '職種名を確認すると「一括実行」で工程①〜⑧を順に走らせます。各工程は個別に再実行できます。';

function allSteps<T>(v: T): Record<StepKey, T> {
  return {
    extract: v,
    analyze: v,
    personas: v,
    copy: v,
    images: v,
    mobile: v,
    hrhacker: v,
    ab: v,
  };
}

export function initialState(): PipelineState {
  return {
    kind: 'free_text',
    sourceKind: 'free_text',
    titleHint: '',
    sourceText: '',
    jobs: [],
    selectedJobIndex: null,
    jobTitle: '',
    jobTitleConfirmed: false,
    facts: null,
    factsText: '',
    category: '',
    analysis: null,
    knowledgeUsed: false,
    personas: [],
    copies: [],
    images: [],
    imagePrompts: [],
    imagePromptsError: '',
    imagesNv: [],
    imagesNumberCheck: '',
    mobile: [],
    hrhacker: null,
    hrhackerCreatedAt: null,
    ab: null,
    status: allSteps<StepStatus>('wait'),
    confirmed: allSteps(false),
    staleSource: {},
    failures: {},
    resultReady: allSteps(false),
    running: false,
    curStep: null,
    normalizing: false,
    statusMessage: null,
    ctlHint: CTL_HINT_INITIAL,
    personaCount: 5,
  };
}

/** 取り込み直しに伴い下流の結果を破棄 (職種確認ゲートもリセット)。旧 `resetResults`。 */
export function resetResults(s: PipelineState): PipelineState {
  return {
    ...s,
    selectedJobIndex: null,
    facts: null,
    factsText: '',
    category: '',
    analysis: null,
    knowledgeUsed: false,
    personas: [],
    copies: [],
    images: [],
    imagePrompts: [],
    imagePromptsError: '',
    imagesNv: [],
    imagesNumberCheck: '',
    mobile: [],
    hrhacker: null,
    hrhackerCreatedAt: null,
    ab: null,
    status: allSteps<StepStatus>('wait'),
    confirmed: allSteps(false),
    staleSource: {},
    failures: {},
    resultReady: allSteps(false),
    jobTitleConfirmed: false,
  };
}

/**
 * 後続工程を「要再実行」にする。旧 `markStaleAfter`。
 * 既に実行済み (done/review/fail/stale) の工程だけが対象。未実行はそのまま待機。
 */
export function markStaleAfter(s: PipelineState, key: StepKey): PipelineState {
  const status = { ...s.status };
  const confirmed = { ...s.confirmed };
  const staleSource = { ...s.staleSource };
  for (const k of staleAfter(key)) {
    if (status[k] === 'wait' || status[k] === 'run') continue;
    status[k] = 'stale';
    confirmed[k] = false;
    // 帯は最初に古くした工程のものを残す (旧: 既に帯があれば重ねない)。
    staleSource[k] ??= key;
  }
  return { ...s, status, confirmed, staleSource };
}

/** ⑧の要約文 (旧 `runAb` の `summary`)。 */
export function abSummary(s: PipelineState): string {
  const parts: string[] = [];
  if (s.category) parts.push('職種カテゴリ: ' + s.category);
  if (s.analysis) {
    parts.push('表面の強み: ' + s.analysis.surface_strengths.join('、'));
    parts.push('裏の強み: ' + s.analysis.hidden_strengths.join('、'));
    parts.push('ボトルネック: ' + s.analysis.bottlenecks.join('、'));
  }
  if (s.personas.length) parts.push('ペルソナ: ' + s.personas.map((p) => p.label).join('、'));
  return parts.join('\n') || s.sourceText.slice(0, 400);
}

/** ⑦の戦略ヒント (旧 `runHrhacker` の `hint`)。 */
export function strategyHint(s: PipelineState): string {
  return s.analysis
    ? [...s.analysis.surface_strengths, ...s.analysis.hidden_strengths].join('、')
    : '';
}

/** 一括実行は「求人取り込み済み」かつ「職種名の確認済み」で解禁。 */
export function canRunAll(s: PipelineState): boolean {
  return !!s.sourceText && !s.running && !s.normalizing && s.jobTitleConfirmed;
}
