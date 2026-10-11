import columns from '../../generated/jobgen/columns.json';
import { parseHrhFields } from './hrhCopy';
export const manualColumns = columns;
export const managementColumns = ['求人id', '店舗id', '職種id'];
export function manualRow(body: string, structured = true): Record<string, string> {
  const first = body.trimStart().split(/\r?\n/, 1)[0] ?? '';
  const heading = /^([^：:]+)[：:]/.exec(first)?.[1]?.trim();
  const fields = structured && heading && columns.includes(heading) ? parseHrhFields(body) : new Map<string, string>();
  const row = Object.fromEntries(columns.map(key => [key, fields.get(key) ?? '']));
  if (!fields.size) row['仕事内容'] = body;
  return row;
}
export const rowBody = (row: Readonly<Record<string, string>>) => columns.map(key => `${key}：${row[key] ?? ''}`).join('\n');
export async function bodyHash(body: string): Promise<string> {
  const bytes = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(body));
  return [...new Uint8Array(bytes)].map(value => value.toString(16).padStart(2, '0')).join('');
}
export function conditionChanges(current: Readonly<Record<string, string>>, baseline: Readonly<Record<string, string>>, row: Readonly<Record<string, string>>): string[] {
  return columns.filter(key => !managementColumns.includes(key) && (/給与|残業|勤務|稼働|タスク|雇用|研修/.test(key) || ['通勤経路', '最寄り駅', '仕事内容', 'キャッチコピー', 'メリット'].includes(key) || /^(自由項目|仕事情報補足)/.test(key) || /給与|月給|時給|日給|年俸|年収|残業|勤務|稼働|雇用|研修|休日|勤務地|休暇|通勤|[0-9０-９].*[円時休]|正社員|契約社員|パート|アルバイト/.test(`${current[key] ?? ''}${baseline[key] ?? ''}${row[key] ?? ''}`)) && (row[key] !== current[key] || row[key] !== baseline[key]));
}
const names: Record<string, string> = {
  求人id: '求人の管理情報', 店舗id: '事業所の管理情報', 職種id: '職種の管理情報', 案件名: '求人の見出し',
  画像1: '掲載画像のアドレス（1枚目）', 画像2: '掲載画像のアドレス（2枚目）', 画像3: '掲載画像のアドレス（3枚目）',
  仕事内容: '仕事の内容', 通勤経路: '勤務地・通勤の案内', 最寄り駅: '近くの駅', キャッチコピー: '紹介の一言', メリット: '働く魅力',
  雇用形態: '雇用の種類', Indeed表示職種名: '募集する職種', 応募資格: '応募に必要な資格・経験', 給与形態: '給与の種類', '基本給与 最小': '給与の下限', '基本給与 最大': '給与の上限',
  タスクの所要時間: '1回の仕事にかかる時間', タスクの単位: '仕事を数える単位', 平均稼働時間: '平均の勤務時間', 平均稼働日数: '平均の勤務日数', 固定残業代: '給与に含まれる残業代', 想定残業時間: '見込まれる残業時間', 給与補足: '給与についての案内',
  '試用・研修の有無': '試用・研修を行うか', '試用・研修時の雇用条件': '試用・研修中の条件', '試用・研修期の雇用形態': '試用・研修中の雇用の種類', '試用・研修期の給与のタイプ': '試用・研修中の給与の種類', '試用・研修期の基本給与 最小': '試用・研修中の給与の下限', '試用・研修期の基本給与 最大': '試用・研修中の給与の上限', '試用・研修期のタスクの所要時間': '試用・研修中の1回の仕事の時間', '試用・研修期のタスクの単位': '試用・研修中の仕事を数える単位', '試用・研修期の平均稼働時間': '試用・研修中の平均勤務時間', '試用・研修期の平均稼働日数': '試用・研修中の平均勤務日数', '試用・研修期の固定残業代': '試用・研修中の給与に含まれる残業代', '試用・研修期の想定残業時間': '試用・研修中の見込まれる残業時間', '試用・研修の詳細情報': '試用・研修についての案内',
  勤務時間: '働く時間・休憩', 勤務時間帯: '働く時間帯', 制作メモ: '社内の制作メモ', 受動喫煙対策: '職場の喫煙対策', 受動喫煙についての補足情報: '喫煙対策についての案内', 応募方法: '応募の方法', 応募後のプロセス: '応募後の流れ', 採用予定人数: '募集する人数', 問い合わせ電話番号: '問い合わせ先の電話', 連絡先メールアドレス: '問い合わせ先のメール', '応募時通知先メールアドレス（カンマ区切りで複数指定可）': '応募の連絡を受け取るメール', フォームの種類: '応募受付の形式', その他の質問1: '応募時に尋ねること（1）', その他の質問2: '応募時に尋ねること（2）', その他の質問3: '応募時に尋ねること（3）', 公開開始日時: '公開を始める日時', 公開終了日時: '公開を終える日時', 公開: '公開するか',
};
for (let i = 1; i <= 4; i++) for (const [prefix, label] of [['自由項目', '補足欄'], ['仕事情報補足', '仕事の案内']] as const) {
  names[`${prefix}${String(i)}のタイトル`] = `${label}${String(i)}の見出し`;
  names[`${prefix}${String(i)}の内容`] = `${label}${String(i)}の本文`;
}
for (let i = 1; i <= 3; i++) for (const [suffix, label] of [['条件', '適用する条件'], ['深夜帯', '深夜勤務の扱い'], ['最小給与', '給与の下限'], ['最大給与', '給与の上限']] as const) names[`条件付き給与${String(i)} ${suffix}`] = `条件別の給与（${String(i)}）・${label}`;
export const manualLabel = (key: string) => names[key] ?? 'その他の案内';
