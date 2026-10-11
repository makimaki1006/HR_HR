import columns from '../../generated/jobgen/columns.json';
import { parseHrhFields } from './hrhCopy';
export interface ComparedJob { id: string; title: string; media: string; body: string | null }
export function salaryAmount(kind: string | null | undefined, value: string | null | undefined): string {
  if (!value?.trim()) return '—';
  const normalized = value.normalize('NFKC').replaceAll(',', '').trim();
  const amount = /^\d+(?:\.\d+)?$/.test(normalized) ? `${Number(normalized).toLocaleString('ja-JP')}円` : value;
  return `${kind?.trim() ? kind : '給与形態不明'} ${amount}`;
}
const labels: Record<string, string> = {
  案件名: '求人名', Indeed表示職種名: '職種', 給与形態: '給与の種類', '基本給与 最小': '給与の下限', '基本給与 最大': '給与の上限',
  通勤経路: '通勤・アクセス', メリット: '仕事の魅力', 平均稼働時間: '平均の勤務時間', 平均稼働日数: '平均の勤務日数', タスクの所要時間: '1回の仕事の時間', タスクの単位: '仕事の単位', 想定残業時間: '想定される残業時間',
  '試用・研修期の給与のタイプ': '研修中の給与の種類', '試用・研修期の基本給与 最小': '研修中の給与の下限', '試用・研修期の基本給与 最大': '研修中の給与の上限',
  '試用・研修期のタスクの所要時間': '研修中の1回の仕事の時間', '試用・研修期のタスクの単位': '研修中の仕事の単位', '試用・研修期の平均稼働時間': '研修中の平均の勤務時間', '試用・研修期の平均稼働日数': '研修中の平均の勤務日数', '試用・研修期の想定残業時間': '研修中の想定される残業時間',
  応募後のプロセス: '応募後の流れ', '応募時通知先メールアドレス（カンマ区切りで複数指定可）': '応募通知の宛先', 公開: '公開状況',
};
export function comparisonLabel(key: string): string {
  if (labels[key]) return labels[key];
  const free = /^(自由項目|仕事情報補足)([1-4])の(タイトル|内容)$/.exec(key);
  if (free) return `${free[1] === '自由項目' ? '補足' : '仕事の案内'}${free[2] ?? ''}の${free[3] === 'タイトル' ? '見出し' : '本文'}`;
  const conditional = /^条件付き給与([1-3]) (条件|深夜帯|最小給与|最大給与)$/.exec(key);
  if (conditional) return `条件に応じた給与${conditional[1] ?? ''}の${({ 条件: '条件', 深夜帯: '深夜の勤務', 最小給与: '下限', 最大給与: '上限' } as Record<string, string>)[conditional[2] ?? ''] ?? ''}`;
  return key;
}
export function compareJobs(jobs: ComparedJob[]) {
  const fields = jobs.map(job => parseHrhFields(job.body ?? ''));
  const fullDifferent = jobs.some(job => job.body === null) || new Set(jobs.map(job => job.body)).size > 1;
  const rows = columns.filter(key => !key.toLowerCase().endsWith('id')).flatMap(key => {
    const values = fields.map(row => row.get(key) ?? '');
    if (!values.some(value => value !== '')) return [];
    return [{ key, label: comparisonLabel(key), values, different: fullDifferent && new Set(values).size > 1 }];
  });
  return { fullDifferent, rows, unstructured: jobs.filter((job, index) => job.body && !fields[index]?.size) };
}
export function comparisonValue(key: string, value: string, body: string | null): string {
  if (!value) return '—';
  const fields = parseHrhFields(body ?? '');
  if (key === '基本給与 最小' || key === '基本給与 最大') return salaryAmount(fields.get('給与形態'), value);
  if (key === '試用・研修期の基本給与 最小' || key === '試用・研修期の基本給与 最大') return salaryAmount(fields.get('試用・研修期の給与のタイプ'), value);
  return value;
}
