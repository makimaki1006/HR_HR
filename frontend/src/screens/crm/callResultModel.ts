// 架電結果の入力欄 (「架電結果の入力」パネル) の純粋な部分。
// 2026-10-14: 試作 (6 択 + 試作の項目。HubSpot に送らない) を、HubSpot に実在する取引プロパティへ保存する欄に置き換えた。
// - 何を入力させるかは「組」(BPO / 新規営業) で決まる。組の定義は RESULT_GROUPS の 1 か所
// - 保存は既存の書き込み経路 (PATCH /api/crm/deals/{id}。項目とステージを 1 回で送る)
// - 入力途中の内容は、このタブの sessionStorage に下書きとして残す (HubSpot に送る前の内容だけ)
import type { MocPropertyDefinition } from './mocProperties';
import { MAX_SELECTED_PER_OBJECT } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import type { FieldChange, FieldDef, FieldKind } from './writeModel';
import { draftToValue, initialDraft, sameValue } from './writeModel';

// ---- 組 (チームごとの項目) ----

export type ResultTeam = 'bpo' | 'sales';

/** 項目の役目。callDate = 空なら今日を入れる / memo = 追記で入れる */
export type ResultFieldRole = 'callDate' | 'nextDate' | 'memo' | 'plain';

export interface ResultFieldSpec {
  /** 取引プロパティの内部名 (画面には出さない) */
  name: string;
  role: ResultFieldRole;
  /** 常に見るもの (大きく手元に)。false は控えめに出す */
  primary: boolean;
}

export interface ResultGroupDef {
  team: ResultTeam;
  /** 画面に出す組の名前 */
  label: string;
  /** HubSpot の owner の teams[].name で、この組に入れるチーム名 */
  hubspotTeams: readonly string[];
  /** 入力欄に出す項目 (この順に出す) */
  fields: readonly ResultFieldSpec[];
  /** ステージのボタンに出す、よく使うステージの ID (いまのパイプラインにあるものだけ出す) */
  quickStages: readonly string[];
}

/** BPO のパイプライン (bpo_リクロジ)。チームで決まらないとき、このパイプラインの取引は BPO の組にする */
export const BPO_PIPELINE_ID = '753186575';

/**
 * 組の定義 (正本はここ 1 か所)。
 * 出典: HubSpot の変更履歴で、架電者本人が通話の前後 30 分に手で更新した割合 (2026-10-14 調査)。
 * 自動で付く項目 (bpo_5・zenkaisuteji・bpo_54 など) はワークフローに任せ、ここには入れない。
 * 将来、管理画面で変えられるようにするときは、役割ごとのプロパティ欄の設定 (ADR-024、src/crm/property_layouts.rs) に載せる
 */
export const RESULT_GROUPS: Readonly<Record<ResultTeam, ResultGroupDef>> = {
  bpo: {
    team: 'bpo', label: 'BPO', hubspotTeams: ['BPO_リクロジ'],
    fields: [
      { name: 'risuto_kadennbi', role: 'callDate', primary: true }, // 架電日
      { name: 'bpo_13', role: 'nextDate', primary: true }, // 次回架電日
      { name: 'bpo_14', role: 'plain', primary: true }, // 次回架電時間
      { name: 'bpo_16', role: 'memo', primary: true }, // タスクメモ
      { name: 'bpo_51', role: 'plain', primary: false }, // 接触
      { name: 'nitteigiri', role: 'plain', primary: false }, // 日程切り
    ],
    // 不在・受付ブロック・不通・担当者ブロック・ニーズなし/無料のみ・日程確保・アポ日確定
    quickStages: ['1095387445', '1095387444', '1095387443', '1095387446', '1095387447', '1319310149', '1095457875'],
  },
  sales: {
    team: 'sales', label: '新規営業', hubspotTeams: ['新規営業'],
    fields: [
      { name: 'risuto_kadennbi', role: 'callDate', primary: true }, // 架電日
      { name: 'risuto_saikadennbi', role: 'nextDate', primary: true }, // 再架電日
      { name: 'saikadenn_zikan', role: 'plain', primary: true }, // 再架電時間
      { name: 'risuto_bikou', role: 'memo', primary: true }, // 激アツメモ
    ],
    // 担当者不在・受付ブロック・TEL_不通・担当者接触/担当者ブロック・アポヨミ・担当者_有料サービス利用経験アリ・TEL_アポ日確定
    quickStages: ['decisionmakerboughtin', 'presentationscheduled', 'qualifiedtobuy', 'closedwon', '1366400580', '122445645', '51997752'],
  },
};

/** 両方の組の項目の内部名 (重複なし)。案件の詳細を読むときに、表示中の項目に足して読む */
export const RESULT_PROPERTY_NAMES: readonly string[] = [...new Set(Object.values(RESULT_GROUPS).flatMap(g => g.fields.map(f => f.name)))];

/**
 * 案件の詳細を読むときの項目: 表示中の項目に、組の項目を足す (架電結果の入力欄が今の値を使う。HubSpot の呼び出しは増えない)。
 * 1 つの型の上限 (100) を超える分は足さない (表示中の項目を優先する)
 */
export function withResultProps(sel: SelectedProps, max: number = MAX_SELECTED_PER_OBJECT): SelectedProps {
  const extra = RESULT_PROPERTY_NAMES.filter(n => !sel.deals.includes(n));
  if (extra.length === 0 || sel.deals.length >= max) return sel;
  return { ...sel, deals: [...sel.deals, ...extra.slice(0, max - sel.deals.length)] };
}

/**
 * 見ている人の組。HubSpot のチームで決め、両方・どちらでもないときは取引のパイプラインで決める
 * (bpo_リクロジ なら BPO、それ以外は新規営業)
 */
export function resultTeamOf(teams: readonly string[] | null | undefined, pipelineId: string | null | undefined): ResultTeam {
  const has = (t: ResultTeam) => (teams ?? []).some(x => RESULT_GROUPS[t].hubspotTeams.includes(x));
  const bpo = has('bpo');
  const sales = has('sales');
  if (bpo && !sales) return 'bpo';
  if (sales && !bpo) return 'sales';
  return pipelineId === BPO_PIPELINE_ID ? 'bpo' : 'sales';
}

/** ボタンに出すステージ。よく使うもののうち、いまのパイプラインにあるもの (無ければ、ステージが少ないときだけ全部) */
export function quickStagesOf(group: ResultGroupDef, stages: readonly { id: string; label: string }[]): { id: string; label: string }[] {
  const quick = group.quickStages.map(id => stages.find(s => s.id === id)).filter((s): s is { id: string; label: string } => s !== undefined);
  if (quick.length > 0) return quick;
  return stages.length <= 8 ? [...stages] : [];
}

// ---- 追記のメモ ----

/** JST の今日の MM/DD (追記の行の頭に付ける) */
export const memoDatePrefix = (today: string): string => `${today.slice(5, 7)}/${today.slice(8, 10)}`;
/** 1 行の項目に足すときの区切り */
export const ONE_LINE_SEPARATOR = ' / ';

/**
 * いまの内容の末尾へ、日付を付けた 1 行を足した値。足すものが空なら null (変えない)。
 * 複数行の項目は改行で、1 行の項目は「 / 」で足す (1 行の項目に改行を入れない)
 */
export function appendMemo(current: string | null | undefined, add: string, kind: FieldKind, today: string): string | null {
  const multiline = kind === 'textarea';
  const text = multiline ? add.replace(/\s+$/u, '').replace(/^\s+/u, '') : add.replace(/\s*\r?\n\s*/gu, ' ').trim();
  if (text === '') return null;
  const line = `${memoDatePrefix(today)} ${text}`;
  const base = (current ?? '').replace(/\s+$/u, '');
  if (base === '') return line;
  return multiline ? `${base}\n${line}` : `${base}${ONE_LINE_SEPARATOR}${line}`;
}

// ---- 下書き ----

/** 入力欄の下書き。stageId '' = ステージは変えない。values は手を入れた項目だけ (メモは足す文) */
export interface ResultDraft {
  stageId: string;
  values: Record<string, string>;
}

export const emptyResultDraft = (): ResultDraft => ({ stageId: '', values: {} });
export const isEmptyDraft = (d: ResultDraft): boolean => d.stageId === '' && Object.keys(d.values).length === 0;

/** 入力欄に出す値。手を入れていなければ HubSpot の今の値 (架電日が空なら今日、メモは空の追記欄) */
export function shownValue(spec: ResultFieldSpec, def: FieldDef, draft: ResultDraft, current: Readonly<Record<string, string | null>>, today: string): string {
  const v = draft.values[spec.name];
  if (v !== undefined) return v;
  if (spec.role === 'memo') return '';
  const cur = initialDraft(def.kind, current[spec.name]);
  if (spec.role === 'callDate' && cur === '') return today;
  return cur;
}

/**
 * 保存する変更 (今の値と違う項目だけ)。base は今の値 (競合の確認に使う)。
 * メモは今の内容の末尾に足した値を送る
 */
export function resultChanges(specs: readonly { spec: ResultFieldSpec; def: FieldDef }[], draft: ResultDraft,
  current: Readonly<Record<string, string | null>>, today: string): FieldChange[] {
  const out: FieldChange[] = [];
  for (const { spec, def } of specs) {
    const base = current[spec.name] ?? null;
    if (spec.role === 'memo') {
      const next = appendMemo(base, draft.values[spec.name] ?? '', def.kind, today);
      if (next !== null) out.push({ object: def.object, name: spec.name, base, value: next });
      continue;
    }
    const value = draftToValue(def.kind, shownValue(spec, def, draft, current, today));
    if (!sameValue(value, draftToValue(def.kind, initialDraft(def.kind, base)))) out.push({ object: def.object, name: spec.name, base, value });
  }
  return out;
}

// ---- 日付 ----

/** 選択肢の値 → HubSpot の表示ラベル。新しく選べない (hidden) 選択肢も含めて探し、無ければ値のまま */
export function optionLabel(def: MocPropertyDefinition | undefined, value: string): string {
  return def?.options.find(o => o.value === value)?.label ?? value;
}

/** JST の今日 (YYYY-MM-DD)。日付の区切りは JST の 0 時 */
export function todayJst(now: number = Date.now()): string {
  return new Date(now + 9 * 3600_000).toISOString().slice(0, 10);
}

/** 次の JST 0 時までのミリ秒 (1 以上) */
export function msUntilNextJstMidnight(now: number = Date.now()): number {
  const day = 86_400_000;
  const jst = now + 9 * 3600_000;
  return day - (((jst % day) + day) % day) || day;
}

/** 実在する暦日の YYYY-MM-DD か (2026-02-30 などは不可) */
export function isCalendarDate(s: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return false;
  const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const t = new Date(Date.UTC(y, mo - 1, d));
  return t.getUTCFullYear() === y && t.getUTCMonth() === mo - 1 && t.getUTCDate() === d;
}

// ---- 下書きの保存 (この画面のタブの sessionStorage。別のタブ・閉じた後には残らない。使えなくても画面は動く) ----

/** v2: 実在のプロパティの下書き (v1 の試作の下書きは読まない) */
export const DRAFT_STORAGE_KEY = 'hrhr.crm.callResult.v2';
/** recorded = この画面から HubSpot に保存した (保存済み・反映待ち) 案件 */
export interface DraftStore { drafts: Record<string, ResultDraft>; recorded: Record<string, true> }
export const emptyStore = (): DraftStore => ({ drafts: {}, recorded: {} });
/** モードを含めた鍵 (実データと架空サンプルの同じ ID を取り違えない) */
export const draftKey = (mode: string, dealId: string) => `${mode}:${dealId}`;

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);
const NAME_RE = /^[A-Za-z0-9_]{1,100}$/;

/**
 * 保存されていた値を検証して戻す。形が違う下書きは捨てる。
 * `user` を渡したら、その人が保存したものだけを戻す (共用の PC で、同じタブで別の人がログインし直しても前の人のメモを見せない)
 */
export function parseStore(raw: string | null, user?: string): DraftStore {
  if (raw === null) return emptyStore();
  let v: unknown;
  try { v = JSON.parse(raw); } catch { return emptyStore(); }
  if (!isObj(v)) return emptyStore();
  if (user !== undefined && (typeof v.user !== 'string' || v.user.toLowerCase() !== user.toLowerCase())) return emptyStore();
  const store = emptyStore();
  if (isObj(v.drafts)) {
    for (const [k, d] of Object.entries(v.drafts)) {
      if (!isObj(d) || typeof d.stageId !== 'string' || !isObj(d.values)) continue;
      const values = Object.entries(d.values);
      if (!values.every(([n, x]) => NAME_RE.test(n) && typeof x === 'string')) continue;
      store.drafts[k] = { stageId: d.stageId, values: Object.fromEntries(values) as Record<string, string> };
    }
  }
  if (isObj(v.recorded)) for (const [k, x] of Object.entries(v.recorded)) if (x === true) store.recorded[k] = true;
  return store;
}

/** ログインしている人 (メールアドレス) が保存した下書きだけを読む。別の人のものは読まない */
export function loadStore(storage: Pick<Storage, 'getItem'> | null, user: string): DraftStore {
  try { return parseStore(storage?.getItem(DRAFT_STORAGE_KEY) ?? null, user); } catch { return emptyStore(); }
}
/** 保存できたら true。sessionStorage が使えない・書けない (容量・設定) ときは false。保存した人も一緒に書く */
export function saveStore(storage: Pick<Storage, 'setItem'> | null, store: DraftStore, user: string): boolean {
  if (storage === null) return false;
  try { storage.setItem(DRAFT_STORAGE_KEY, JSON.stringify({ user, ...store })); return true; } catch { return false; }
}
export function sessionStorageOrNull(): Storage | null {
  try { return typeof window === 'undefined' ? null : window.sessionStorage; } catch { return null; }
}

const without = <T,>(rec: Record<string, T>, key: string): Record<string, T> => Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));
/** 下書きを入れ替える。空の下書きは消す (何も入れていない案件を残さない) */
export function putDraft(s: DraftStore, key: string, d: ResultDraft): DraftStore {
  return { ...s, drafts: isEmptyDraft(d) ? without(s.drafts, key) : { ...s.drafts, [key]: d } };
}
/** 「入力を消す」: 下書きを消す (保存済みの印は残す) */
export function clearDraftEntry(s: DraftStore, key: string): DraftStore {
  return { ...s, drafts: without(s.drafts, key) };
}
/** HubSpot に保存した (保存済み・反映待ち): 下書きを消し、保存済みの印を付ける */
export function markRecorded(s: DraftStore, key: string): DraftStore {
  return { drafts: without(s.drafts, key), recorded: { ...s.recorded, [key]: true } };
}

/** 一覧の並びで、current の次にある未記録の案件 (末尾まで無ければ先頭から)。無ければ null */
export function nextUnrecorded(ids: readonly string[], current: string, isRecorded: (id: string) => boolean): string | null {
  const i = ids.indexOf(current);
  const order = i === -1 ? ids : [...ids.slice(i + 1), ...ids.slice(0, i)];
  return order.find(id => id !== current && !isRecorded(id)) ?? null;
}
