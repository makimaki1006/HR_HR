import { describe, expect, it } from 'vitest';
import type { FeatureChart } from '../../generated/FeatureChart';
import { orderRows, pctText, readingSentence } from './featureModel';

const row = (label: string, n: number, popular: number) => ({ label, n, popular, rest_n: 100, rest_popular: 40, uncertain: false, note: null });
const chart = (rows: FeatureChart['rows'], over: Partial<FeatureChart> = {}): FeatureChart => ({
  id: 'main', title: 't', total: 0, total_popular: 0, ordered: false, band: false, rows, notes: [], ...over,
});

describe('featureModel', () => {
  it('割合の高い順、ordered は元の順、n=0 は最後', () => {
    const rows = [row('a', 10, 2), row('b', 0, 0), row('c', 10, 9)];
    expect(orderRows(chart(rows)).map((r) => r.label)).toEqual(['c', 'a', 'b']);
    expect(orderRows(chart(rows, { ordered: true })).map((r) => r.label)).toEqual(['a', 'b', 'c']);
    expect(pctText(0, 0)).toBe('—');
  });
  it('見方の文は20件以上の最初の行から作る', () => {
    const rows = [row('少', 5, 5), row('駅近5分以内', 34, 25)];
    expect(readingSentence(chart(rows), rows)).toBe(
      '見方: 緑の棒は「駅近5分以内」と載っている求人34件のうち、人気・超人気が25件(74%)という意味です。青の棒は、載っていない求人100件のうち40件(40%)です。',
    );
    expect(readingSentence(chart(rows, { band: true }), rows)).toContain('緑の棒は「駅近5分以内」の求人34件');
    expect(readingSentence(chart([row('少', 5, 5)]), [row('少', 5, 5)])).toBeNull();
  });
});
