import type { FeatureChart } from '../../generated/FeatureChart';
import type { FeatureRow } from '../../generated/FeatureRow';
import { MISSING } from './format';

/** この件数未満の比較は薄い色で「(少なめ)」と出す。 */
export const FEW_N = 20;

/** popular / n を四捨五入した整数%。n が 0 のときは null。 */
export function ratePct(popular: number, n: number): number | null {
  return n === 0 ? null : Math.round((popular / n) * 100);
}

export function pctText(popular: number, n: number): string {
  const p = ratePct(popular, n);
  return p === null ? MISSING : `${String(p)}%`;
}

export const isFew = (n: number): boolean => n < FEW_N;

/** 緑の棒 (載っている求人 / この区分) の割合。並べ替え用に小数のまま返す。null は最後。 */
function rawRate(r: FeatureRow): number {
  return r.n === 0 ? -1 : r.popular / r.n;
}

/** 表示順。ordered なら渡された順、そうでなければ割合の高い順 (同率は元の順)。 */
export function orderRows(chart: FeatureChart): FeatureRow[] {
  const rows = [...chart.rows];
  if (chart.ordered) return rows;
  return rows
    .map((r, i) => ({ r, i }))
    .sort((a, b) => rawRate(b.r) - rawRate(a.r) || a.i - b.i)
    .map((x) => x.r);
}

/** 凡例の 2 つの語。帯のグラフは「この区分 / ほかの区分」。 */
export function legendLabels(chart: FeatureChart): [string, string] {
  return chart.band
    ? ['この区分の求人', 'ほかの区分の求人']
    : ['その項目が載っている求人', '載っていない求人'];
}

/** 「見方」の 1 文。基準にするのは表示順で最初の、件数が足りる行。無ければ null。 */
export function readingSentence(chart: FeatureChart, rows: readonly FeatureRow[]): string | null {
  const r = rows.find((x) => !isFew(x.n));
  if (!r) return null;
  const green = `${String(r.n)}件のうち、人気・超人気が${String(r.popular)}件(${pctText(r.popular, r.n)})という意味です。`;
  const blue = `${String(r.rest_n)}件のうち${String(r.rest_popular)}件(${pctText(r.rest_popular, r.rest_n)})です。`;
  if (chart.band) {
    const subject = r.label.startsWith('「') ? r.label : `「${r.label}」の`;
    return `見方: 緑の棒は${subject}求人${green}青の棒は、ほかの区分の求人${blue}`;
  }
  return `見方: 緑の棒は「${r.label}」と載っている求人${green}青の棒は、載っていない求人${blue}`;
}
