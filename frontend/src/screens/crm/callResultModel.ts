// 架電結果の下書き (HubSpot にはまだ書き込まない)。
// 項目・規則の出典は claudedocs/CRM_CALL_RESULT_FORM_2026-10-08.md。
// - 「今回の結果」は UI 専用 (HubSpot のプロパティではない)。CALL_RESULTS (callModel.ts) の 6 択
// - 選択肢は HubSpot の定義 (実データでは GET /api/crm/metadata、架空サンプルでは mocProperties.ts) から読む
// - 書き込み用の PATCH ペイロードは toHubSpotPatch で作るが、この PR ではどこにも送らない
import { CALL_RESULTS } from './callModel';
import type { CallResult } from './callModel';
import type { MocPropertyDefinition } from './mocProperties';

export { CALL_RESULTS };
export type { CallResult };

export interface ResultDraft {
  outcome: CallResult | '';
  /** bpo_40 接触結果 (受付 / 担当者) */
  spokeTo: string;
  /** bpo_42 担当者会話温度感 */
  interest: string;
  /** bpo_45 次アクション種別 */
  nextAction: string;
  /** bpo_13 次回架電日 (JST の暦日 YYYY-MM-DD) */
  nextCallDate: string;
  /** bpo_14 次回架電時間 (選択肢の値 '9:15' など) */
  nextCallTime: string;
  /** bpo_16 タスクメモ */
  memo: string;
  /** bpo_10 不通時チェック */
  unreachable: string;
  /** bpo_57 その他理由 (bpo_10 = その他 のとき) */
  unreachableOther: string;
  /** bpo_3 架電禁止理由 */
  stopReason: string;
  /** bpo_4 ブロック理由 */
  blockReason: string;
  /** bpo_23 商談予定日 */
  apptDate: string;
  /** bpo__ 商談予定時間（bpo用） */
  apptTime: string;
  /** bpo_33 商談方法（bpo用） */
  apptMethod: string;
}
export type DraftField = Exclude<keyof ResultDraft, 'outcome'>;

export const emptyResultDraft = (): ResultDraft => ({
  outcome: '', spokeTo: '', interest: '', nextAction: '', nextCallDate: '', nextCallTime: '', memo: '',
  unreachable: '', unreachableOther: '', stopReason: '', blockReason: '', apptDate: '', apptTime: '', apptMethod: '',
});

/** 下書きの項目 → HubSpot の Deal プロパティの内部名。ここに無いものは PATCH に入れない (許可リスト) */
export const FIELD_PROPERTY: Readonly<Record<DraftField, string>> = {
  spokeTo: 'bpo_40', interest: 'bpo_42', nextAction: 'bpo_45', nextCallDate: 'bpo_13', nextCallTime: 'bpo_14',
  memo: 'bpo_16', unreachable: 'bpo_10', unreachableOther: 'bpo_57', stopReason: 'bpo_3', blockReason: 'bpo_4',
  apptDate: 'bpo_23', apptTime: 'bpo__', apptMethod: 'bpo_33',
};
export const RESULT_PROPERTY_ALLOWLIST: readonly string[] = Object.values(FIELD_PROPERTY);

/** 定義が手元に無いとき (bpo_57 は /api/crm/metadata の許可リスト外) の見出し。HubSpot のラベルと同じ */
export const FALLBACK_LABELS: Readonly<Record<string, string>> = { bpo_57: 'その他理由' };

/** 選択肢を HubSpot の定義から読む項目。実データで 1 つでも定義が無ければ入力欄を出さない (架空の選択肢で代用しない) */
export const ENUM_FIELDS = ['spokeTo', 'interest', 'nextAction', 'nextCallTime', 'unreachable', 'blockReason', 'apptTime', 'apptMethod'] as const satisfies readonly DraftField[];
export const REQUIRED_DEFINITIONS: readonly string[] = ENUM_FIELDS.map(f => FIELD_PROPERTY[f]);

const DATE_FIELDS: ReadonlySet<DraftField> = new Set(['nextCallDate', 'apptDate']);

export const TEXT_LIMITS: Readonly<Partial<Record<DraftField, number>>> = { memo: 2000, stopReason: 200, unreachableOther: 200 };

export const SPOKE_TO_PERSON = '担当者';
export const NEXT_ACTION_RECALL = '再架電';
export const UNREACHABLE_OTHER = 'その他';

/** 話した相手がいる結果 (接触結果・温度感を聞く) */
const WITH_PARTNER: ReadonlySet<CallResult> = new Set(['connected', 'callback', 'appointment', 'do_not_call']);
/** 次アクション・次回架電を入れない結果 */
const NO_NEXT: ReadonlySet<CallResult> = new Set(['do_not_call', 'wrong_number']);

/** いまの下書きで表示・検証・送信の対象になる項目。隠れた項目の値は残っていても送らない */
export function activeFields(d: ResultDraft): Set<DraftField> {
  const f = new Set<DraftField>(['memo']);
  const o = d.outcome;
  if (o === '') return f;
  if (WITH_PARTNER.has(o)) {
    f.add('spokeTo');
    if (d.spokeTo === SPOKE_TO_PERSON) f.add('interest');
  }
  if (!NO_NEXT.has(o)) { f.add('nextAction'); f.add('nextCallDate'); f.add('nextCallTime'); }
  if (o === 'no_answer' || o === 'wrong_number') {
    f.add('unreachable');
    if (d.unreachable === UNREACHABLE_OTHER) f.add('unreachableOther');
  }
  if (o === 'do_not_call') { f.add('stopReason'); f.add('blockReason'); }
  if (o === 'appointment') { f.add('apptDate'); f.add('apptTime'); f.add('apptMethod'); }
  return f;
}

/** 結果を選び直したときの下書き。再架電の約束なら次アクションを「再架電」にする (空のときだけ) */
export function withOutcome(d: ResultDraft, outcome: CallResult): ResultDraft {
  const next = { ...d, outcome };
  if (outcome === 'callback' && next.nextAction === '') next.nextAction = NEXT_ACTION_RECALL;
  return next;
}

/** JST の今日 (YYYY-MM-DD)。日付の区切りは JST の 0 時 */
export function todayJst(now: number = Date.now()): string {
  return new Date(now + 9 * 3600_000).toISOString().slice(0, 10);
}

/** 実在する暦日の YYYY-MM-DD か (2026-02-30 などは不可) */
export function isCalendarDate(s: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return false;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const t = new Date(Date.UTC(y, mo - 1, d));
  return t.getUTCFullYear() === y && t.getUTCMonth() === mo - 1 && t.getUTCDate() === d;
}

/** 新しく選べる選択肢 (hidden は除く) */
export function selectableOptions(def: MocPropertyDefinition | undefined): { label: string; value: string }[] {
  return (def?.options ?? []).filter(o => !o.hidden).map(o => ({ label: o.label, value: o.value }));
}

export type DraftErrors = Partial<Record<DraftField | 'outcome', string>>;

/**
 * 下書きの検証。項目ごとのメッセージを返す (空なら記録できる)。
 * `defs` は Deal プロパティの定義 (選択肢の値の確認に使う)、`today` は JST の YYYY-MM-DD
 */
export function validateResultDraft(d: ResultDraft, defs: Record<string, MocPropertyDefinition>, today: string): DraftErrors {
  const e: DraftErrors = {};
  if (d.outcome === '') { e.outcome = '今回の結果を選んでください。'; return e; }
  const active = activeFields(d);
  const has = (f: DraftField) => active.has(f) && d[f].trim() !== '';

  for (const f of ENUM_FIELDS) {
    if (!has(f)) continue;
    if (!selectableOptions(defs[FIELD_PROPERTY[f]]).some(o => o.value === d[f])) e[f] = '選択肢にない値です。選び直してください。';
  }
  for (const [f, max] of Object.entries(TEXT_LIMITS) as [DraftField, number][]) {
    if (active.has(f) && d[f].length > max) e[f] = `${String(max)} 文字以内で入力してください(いま ${String(d[f].length)} 文字)。`;
  }
  for (const f of DATE_FIELDS) {
    if (!has(f)) continue;
    if (!isCalendarDate(d[f])) e[f] = '日付を確認してください。';
    else if (d[f] < today) e[f] = '今日以降の日付を入れてください。';
  }

  if (active.has('nextCallDate')) {
    const needNext = d.outcome === 'callback' || (active.has('nextAction') && d.nextAction === NEXT_ACTION_RECALL);
    const date = has('nextCallDate');
    const time = has('nextCallTime');
    if (needNext || date || time) {
      if (!date) e.nextCallDate ??= needNext ? '次回架電日を入れてください(再架電のとき必須)。' : '次回架電日も入れてください(時間とセット)。';
      if (!time) e.nextCallTime ??= needNext ? '次回架電時間を選んでください(再架電のとき必須)。' : '次回架電時間も選んでください(日付とセット)。';
    }
  }
  if (d.outcome === 'appointment') {
    if (!has('apptDate')) e.apptDate ??= '商談予定日を入れてください。';
    if (!has('apptTime')) e.apptTime ??= '商談予定時間を選んでください。';
    if (!has('apptMethod')) e.apptMethod ??= '商談方法を選んでください。';
  }
  if (d.outcome === 'do_not_call' && !has('stopReason')) e.stopReason ??= '架電禁止理由を入れてください。';
  if (d.outcome === 'wrong_number' && !has('unreachable')) e.unreachable ??= '不通時チェックを選んでください。';
  if (active.has('unreachableOther') && !has('unreachableOther')) e.unreachableOther ??= '「その他」の理由を入れてください。';
  return e;
}

export const isDraftValid = (d: ResultDraft, defs: Record<string, MocPropertyDefinition>, today: string) =>
  Object.keys(validateResultDraft(d, defs, today)).length === 0;

export interface HubSpotDealPatch { properties: Record<string, string> }

/**
 * 有効な下書き → 将来の HubSpot Deal PATCH の本文 `{ properties: { 内部名: 文字列 } }`。
 * 無効な下書きは null。この PR ではどこにも送らない (書き込みは次の PR)。
 * - 表示・入力の対象になっている項目のうち、空でないものだけ
 * - 日付 (bpo_13 / bpo_23) は JST の暦日をそのまま YYYY-MM-DD (HubSpot の date は日付だけ。Date に通すとずれる)
 * - 時間・選択肢は選択肢の値 (表示ラベルではない)
 * - 許可リスト (FIELD_PROPERTY) 以外の内部名は入れない
 */
export function toHubSpotPatch(d: ResultDraft, defs: Record<string, MocPropertyDefinition>, today: string): HubSpotDealPatch | null {
  if (!isDraftValid(d, defs, today)) return null;
  const active = activeFields(d);
  const properties: Record<string, string> = {};
  for (const [field, name] of Object.entries(FIELD_PROPERTY) as [DraftField, string][]) {
    if (!active.has(field) || !RESULT_PROPERTY_ALLOWLIST.includes(name)) continue;
    const value = field === 'memo' ? d.memo.replace(/\s+$/u, '') : d[field].trim();
    if (value === '') continue;
    properties[name] = value;
  }
  return { properties };
}

/** 折りたたんだときの 1 行 */
export function draftSummary(d: ResultDraft, defs: Record<string, MocPropertyDefinition>): string {
  if (d.outcome === '') return '結果は未選択';
  const label = (f: DraftField) => selectableOptions(defs[FIELD_PROPERTY[f]]).find(o => o.value === d[f])?.label ?? d[f];
  const active = activeFields(d);
  const parts: string[] = [CALL_RESULTS[d.outcome]];
  if (active.has('spokeTo') && d.spokeTo) parts.push(label('spokeTo'));
  if (active.has('nextCallDate') && d.nextCallDate) parts.push(`次回 ${d.nextCallDate.slice(5).replace('-', '/')}${d.nextCallTime ? ` ${d.nextCallTime}` : ''}`);
  if (active.has('apptDate') && d.apptDate) parts.push(`商談 ${d.apptDate.slice(5).replace('-', '/')}${d.apptTime ? ` ${d.apptTime}` : ''}`);
  if (d.memo.trim()) parts.push('メモあり');
  return parts.join(' · ');
}

// ---- 下書きの保存 (このブラウザのタブの sessionStorage。使えなくても画面は動く) ----

export const DRAFT_STORAGE_KEY = 'hrhr.crm.callResult.v1';
export interface DraftStore { drafts: Record<string, ResultDraft>; recorded: Record<string, true> }
export const emptyStore = (): DraftStore => ({ drafts: {}, recorded: {} });
/** モードを含めた鍵 (実データと架空サンプルの同じ ID を取り違えない) */
export const draftKey = (mode: string, dealId: string) => `${mode}:${dealId}`;

const OUTCOMES = Object.keys(CALL_RESULTS) as CallResult[];
const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** 保存されていた値を検証して戻す。形が違う下書きは捨てる */
export function parseStore(raw: string | null): DraftStore {
  if (raw === null) return emptyStore();
  let v: unknown;
  try { v = JSON.parse(raw); } catch { return emptyStore(); }
  if (!isObj(v)) return emptyStore();
  const store = emptyStore();
  if (isObj(v.drafts)) {
    for (const [k, d] of Object.entries(v.drafts)) {
      if (!isObj(d)) continue;
      const draft = emptyResultDraft();
      let ok = true;
      for (const f of Object.keys(draft) as (keyof ResultDraft)[]) {
        const x = d[f];
        if (x === undefined) continue;
        if (typeof x !== 'string') { ok = false; break; }
        if (f === 'outcome') { if (x !== '' && !OUTCOMES.includes(x as CallResult)) { ok = false; break; } draft.outcome = x as CallResult | ''; }
        else draft[f] = x;
      }
      if (ok) store.drafts[k] = draft;
    }
  }
  if (isObj(v.recorded)) for (const [k, x] of Object.entries(v.recorded)) if (x === true) store.recorded[k] = true;
  return store;
}

export function loadStore(storage: Pick<Storage, 'getItem'> | null): DraftStore {
  try { return parseStore(storage?.getItem(DRAFT_STORAGE_KEY) ?? null); } catch { return emptyStore(); }
}
export function saveStore(storage: Pick<Storage, 'setItem'> | null, store: DraftStore): void {
  try { storage?.setItem(DRAFT_STORAGE_KEY, JSON.stringify(store)); } catch { /* 保存できない環境では画面の中だけで持つ */ }
}
export function sessionStorageOrNull(): Storage | null {
  try { return typeof window === 'undefined' ? null : window.sessionStorage; } catch { return null; }
}

/** 下書きが空 (何も入れていない) か */
export const isEmptyDraft = (d: ResultDraft) => (Object.keys(d) as (keyof ResultDraft)[]).every(k => d[k] === '');

const without = <T,>(rec: Record<string, T>, key: string): Record<string, T> => Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));
/** 下書きを入れ替える。空の下書きは消す (何も入れていない案件を残さない) */
export function putDraft(s: DraftStore, key: string, d: ResultDraft): DraftStore {
  return { ...s, drafts: isEmptyDraft(d) ? without(s.drafts, key) : { ...s.drafts, [key]: d } };
}
/** 「下書きを消す」: 下書きと記録済みの印の両方を消す */
export function clearDraftEntry(s: DraftStore, key: string): DraftStore {
  return { drafts: without(s.drafts, key), recorded: without(s.recorded, key) };
}
export function markRecorded(s: DraftStore, key: string): DraftStore {
  return { ...s, recorded: { ...s.recorded, [key]: true } };
}

/** 一覧の並びで、current の次にある未記録の案件 (末尾まで無ければ先頭から)。無ければ null */
export function nextUnrecorded(ids: readonly string[], current: string, isRecorded: (id: string) => boolean): string | null {
  const i = ids.indexOf(current);
  const order = i === -1 ? ids : [...ids.slice(i + 1), ...ids.slice(0, i)];
  return order.find(id => id !== current && !isRecorded(id)) ?? null;
}
