import columns from '../../generated/jobgen/columns.json';
import { displayCode } from './copyDisplay';
/** HRハッカー compose_copy_body の列を、読みやすい求人票に組み直す。 */
export interface BodySection { heading: string; text: string }
const labels: Record<string, string> = {
  案件名: '案件名', 仕事内容: '仕事内容', 通勤経路: '通勤・アクセス', 最寄り駅: '最寄り駅', キャッチコピー: 'キャッチコピー', メリット: '仕事の魅力',
  雇用形態: '雇用形態', Indeed表示職種名: '職種', 応募資格: '応募資格', 給与形態: '給与', '基本給与 最小': '給与', '基本給与 最大': '給与',
  タスクの所要時間: '1回の仕事の時間', タスクの単位: '仕事の単位', 平均稼働時間: '平均の勤務時間', 平均稼働日数: '平均の勤務日数',
  固定残業代: '固定残業代', 想定残業時間: '想定される残業時間', 給与補足: '給与について',
  '試用・研修の有無': '試用・研修', '試用・研修時の雇用条件': '雇用条件', '試用・研修期の雇用形態': '雇用形態', '試用・研修期の給与のタイプ': '給与',
  '試用・研修期の基本給与 最小': '給与', '試用・研修期の基本給与 最大': '給与', '試用・研修期のタスクの所要時間': '1回の仕事の時間', '試用・研修期のタスクの単位': '仕事の単位',
  '試用・研修期の平均稼働時間': '平均の勤務時間', '試用・研修期の平均稼働日数': '平均の勤務日数', '試用・研修期の固定残業代': '固定残業代', '試用・研修期の想定残業時間': '想定される残業時間', '試用・研修の詳細情報': '詳細',
  勤務時間: '勤務時間', 勤務時間帯: '勤務時間帯', 受動喫煙対策: '受動喫煙対策', 受動喫煙についての補足情報: '喫煙に関する案内', 応募方法: '応募方法', 応募後のプロセス: '応募後の流れ', 採用予定人数: '採用予定人数',
};
/** 本文の見出しとして知っている語（HRハッカーの列名と、求人票での呼び名）。 */
export const knownHeadings: ReadonlySet<string> = new Set([...columns, ...Object.values(labels)]);
const conditional = /^条件付き給与([1-3])(?:の|\s*)(条件|深夜帯|最小給与|最大給与)$/;
/** Choose the longest forward column sequence, anchored at the first field.
 * Equal sequences for the same column prefer the later occurrence, preserving
 * column-like lines in an earlier multiline value. The source format is unescaped.
 */
export function parseHrhFields(body: string): Map<string, string> {
  const lines = body.split('\n');
  const offsets: number[] = []; let offset = 0;
  for (const line of lines) { offsets.push(offset); offset += line.length + 1; }
  const candidates: { line: number; rank: number; start: number; length: number; next: number | null }[] = [];
  lines.forEach((line, index) => {
    const match = /^([^：:]+)[：:]([\s\S]*)$/.exec(line);
    const rank = columns.indexOf(match?.[1]?.trim() ?? '');
    if (match && rank >= 0) candidates.push({ line: index, rank, start: (offsets[index] ?? 0) + (match[1]?.length ?? 0) + 1, length: 1, next: null });
  });
  const best: (number | undefined)[] = Array.from({ length: columns.length });
  for (let i = candidates.length - 1; i >= 0; i--) {
    const candidate = candidates[i]; if (!candidate) continue;
    for (let rank = candidate.rank + 1; rank < columns.length; rank++) {
      const next = best[rank]; const following = next === undefined ? undefined : candidates[next];
      if (following && following.length + 1 > candidate.length && next !== undefined) { candidate.length = following.length + 1; candidate.next = next; }
    }
    const existing = best[candidate.rank];
    if (existing === undefined || candidate.length > (candidates[existing]?.length ?? 0)) best[candidate.rank] = i;
  }
  const fields = new Map<string, string>();
  let index: number | null = candidates.length ? 0 : null;
  while (index !== null) {
    const candidate = candidates[index]; if (!candidate) break;
    let end = candidate.next === null ? body.length : offsets[candidates[candidate.next]?.line ?? lines.length] ?? body.length;
    // 次の項目との区切り一つだけを除き、値内の空白・改行は原文のまま保持する。
    if (candidate.next !== null && body[end - 1] === '\n') { end--; if (body[end - 1] === '\r') end--; }
    fields.set(columns[candidate.rank] ?? '', body.slice(candidate.start, end));
    index = candidate.next;
  }
  return fields;
}
const money = (value: string) => /^\d+(?:\.\d+)?$/.test(value.normalize('NFKC').replace(/,/g, ''))
  ? `${Number(value.normalize('NFKC').replace(/,/g, '')).toLocaleString('ja-JP')}円` : value;
export function composeSalary(kind: string, min: string, max: string): string {
  const range = min ? `${money(min)}${max ? min === max ? '' : `〜${money(max)}` : '〜'}` : max ? `${money(max)}まで` : '金額は未取得';
  return `${displayCode('給与形態', kind) || '給与形態は不明'} ${range}`;
}
export function hrhCopySections(body: string): BodySection[] {
  if (!body.trim()) return [];
  const fields = parseHrhFields(body);
  // Earlier saved plain descriptions are still readable.
  if (!fields.size) return [{ heading: '仕事内容', text: body }];
  // 読みやすさのための整形は表示だけに限定する。編集・保存用の値は加工しない。
  const get = (key: string) => displayCode(key, (fields.get(key) ?? '').trim());
  const sections: BodySection[] = [];
  const add = (heading: string, text: string) => { if (text.trim()) sections.push({ heading, text }); };
  const simple = (keys: string[]) => { for (const key of keys) add(labels[key] ?? 'その他', get(key)); };
  const pairs = (prefix: string) => { for (let n = 1; n <= 4; n++) { const heading = get(`${prefix}${String(n)}のタイトル`); const text = get(`${prefix}${String(n)}の内容`); if (heading || text) add(heading || 'その他', text || '内容は未取得'); } };
  const workTime = (prefix = '') => {
    const lines: string[] = [];
    const time = get(`${prefix}タスクの所要時間`); const unit = get(`${prefix}タスクの単位`);
    if (time || unit) lines.push(`1回の仕事の時間：${[time, unit].filter(Boolean).join(' ')}`);
    for (const key of ['平均稼働時間', '平均稼働日数', '固定残業代', '想定残業時間']) {
      const value = get(`${prefix}${key}`); if (value) lines.push(`${labels[key] ?? ''}：${key === '固定残業代' ? money(value) : value}`);
    }
    return lines;
  };
  simple(['案件名', 'キャッチコピー', '仕事内容', '通勤経路', '最寄り駅', 'メリット']);
  pairs('仕事情報補足'); simple(['雇用形態', 'Indeed表示職種名', '応募資格']);
  if (get('給与形態') || get('基本給与 最小') || get('基本給与 最大')) add('給与', composeSalary(get('給与形態'), get('基本給与 最小'), get('基本給与 最大')));
  const payDetails = workTime();
  for (let n = 1; n <= 3; n++) {
    const conditionFields = [...fields].map(([key, value]) => [key, value.trim()] as const).filter(([key]) => conditional.exec(key)?.[1] === String(n));
    const value = (suffix: string) => displayCode(`条件付き給与${String(n)} ${suffix}`, conditionFields.find(([key]) => conditional.exec(key)?.[2] === suffix)?.[1] ?? '');
    if (conditionFields.some(([, text]) => text)) {
      const amount = value('最小給与') || value('最大給与') ? composeSalary(get('給与形態'), value('最小給与'), value('最大給与')) : '';
      payDetails.push([value('条件') || '条件に応じた給与', value('深夜帯') ? `深夜の勤務：${value('深夜帯')}` : '', amount].filter(Boolean).join(' ／ '));
    }
  }
  if (get('給与補足')) payDetails.push(get('給与補足'));
  add('給与・勤務の補足', payDetails.join('\n'));
  const trial: string[] = [];
  for (const key of ['試用・研修の有無', '試用・研修時の雇用条件', '試用・研修期の雇用形態']) { if (get(key)) trial.push(`${labels[key] ?? ''}：${get(key)}`); }
  if (get('試用・研修期の給与のタイプ') || get('試用・研修期の基本給与 最小') || get('試用・研修期の基本給与 最大')) trial.push(`研修中の給与：${composeSalary(get('試用・研修期の給与のタイプ'), get('試用・研修期の基本給与 最小'), get('試用・研修期の基本給与 最大'))}`);
  trial.push(...workTime('試用・研修期の'));
  if (get('試用・研修の詳細情報')) trial.push(get('試用・研修の詳細情報'));
  add('試用・研修', trial.join('\n'));
  simple(['勤務時間', '勤務時間帯']); pairs('自由項目'); simple(['受動喫煙対策', '受動喫煙についての補足情報', '応募方法', '応募後のプロセス', '採用予定人数']);
  return sections;
}
/** Body used by both timeline and comparison. Display uses the exact same sections. */
export const composeHrhBody = (body: string) => hrhCopySections(body).map(section => `${section.heading}：${section.text}`).join('\n\n');
