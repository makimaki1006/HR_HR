// 記録欄 (台本の「記録する欄」)。選択肢の欄は選択肢、職種・課題は短い文。どの欄にも「聞けていない」を選べる。

import type { HearingRecord } from '../../generated/HearingRecord';
import type { RecordField } from '../../generated/RecordField';

export type Slot = 'hiring_3m' | 'hiring_later' | 'issue' | 'urgency' | 'consider' | 'budget' | 'decider' | 'attend';

export interface FieldDef {
  slot: Slot;
  group: string;
  label: string;
  /** 選択肢の欄。無ければ文で書く欄 */
  options?: readonly { value: string; label: string }[];
  placeholder?: string;
}

export const FIELDS: readonly FieldDef[] = [
  { slot: 'hiring_3m', group: '① 時期と人数', label: '3か月以内の職種と人数', placeholder: '例: 介護職 2名' },
  { slot: 'hiring_later', group: '① 時期と人数', label: 'それ以外の募集', placeholder: '例: 看護師 来年の春' },
  { slot: 'issue', group: '② 課題と意欲', label: '課題', placeholder: '例: 応募が来ても面接に来ない' },
  {
    slot: 'urgency',
    group: '② 課題と意欲',
    label: '解決の早さ',
    options: [
      { value: 'early', label: '早く解決したい' },
      { value: 'not_so', label: 'そこまでではない' },
      { value: 'none', label: '困っていない' },
    ],
  },
  {
    slot: 'consider',
    group: '③ 検討と予算',
    label: '検討',
    options: [
      { value: 'positive', label: '前向き' },
      { value: 'depends', label: '内容と金額次第' },
      { value: 'difficult', label: '難しい' },
      { value: 'cannot_decide', label: '自分では決められない' },
    ],
  },
  {
    slot: 'budget',
    group: '③ 検討と予算',
    label: '予算',
    options: [
      { value: 'yes', label: 'ある' },
      { value: 'no', label: '無い' },
      { value: 'refused', label: '答えなかった' },
    ],
  },
  {
    slot: 'decider',
    group: '④ 決める人と同席',
    label: '決める人',
    options: [
      { value: 'self', label: '本人' },
      { value: 'self_plus_one', label: '本人 (もう 1 人に一声かける)' },
      { value: 'boss', label: '上の人' },
      { value: 'hq', label: '本社・別部署' },
    ],
  },
  {
    slot: 'attend',
    group: '④ 決める人と同席',
    label: '当日の同席',
    options: [
      { value: 'yes', label: '来る' },
      { value: 'no', label: '来ない' },
      { value: 'undecided', label: '未定' },
    ],
  },
];

export interface FieldState {
  notHeard: boolean;
  value: string;
}
export type RecordState = Record<Slot, FieldState>;

export function emptyRecord(): RecordState {
  const r = {} as RecordState;
  for (const f of FIELDS) r[f.slot] = { notHeard: false, value: '' };
  return r;
}

function toField(s: FieldState): RecordField {
  if (s.notHeard) return { mode: 'not_heard', value: '' };
  const v = s.value.trim();
  return v === '' ? { mode: 'empty', value: '' } : { mode: 'value', value: v };
}

export function toRequestRecord(r: RecordState): HearingRecord {
  return {
    hiring_3m: toField(r.hiring_3m),
    hiring_later: toField(r.hiring_later),
    issue: toField(r.issue),
    urgency: toField(r.urgency),
    consider: toField(r.consider),
    budget: toField(r.budget),
    decider: toField(r.decider),
    attend: toField(r.attend),
  };
}

/** 空欄のままの欄の名前 */
export function blankLabels(r: RecordState): string[] {
  return FIELDS.filter((f) => toField(r[f.slot]).mode === 'empty').map((f) => f.label);
}

export const HEAT_OPTIONS = [
  { value: 'hot', label: '熱い' },
  { value: 'mixed', label: 'どちらとも言えない' },
  { value: 'cold', label: '冷めている' },
] as const;

export const HEAT_LABEL: Record<string, string> = { hot: '熱い', mixed: 'どちらとも言えない', cold: '冷めている' };

/** 台本の 4 つの聞き方 (練習中に横に出す) */
export const SCRIPT_LINES: readonly { group: string; line: string }[] = [
  { group: '① 時期と人数', line: '直近3か月以内で、採用が必要な職種はありますか？ それは何名くらいですか？ それ以外で、先に考えている募集はありますか？' },
  { group: '② 課題と意欲', line: '今の採用で困っていることと、どのくらい早く解決したいかを教えていただけますか？' },
  { group: '③ 検討と予算', line: '弊社は月額定額のサービスです。その形でしたら、ご検討いただけそうですか？' },
  { group: '④ 決める人と同席', line: '最終的にお決めになる方と、当日ご同席いただけるかを伺えますか？' },
];
