import { describe, expect, it } from 'vitest';
import { applyReasonFixture, REASON_OPTIONS, row } from './applyReason.testFixtures';
import { buildCrosstab, columnsOf, formatShare, labelsOf } from './applyReasonCrosstabModel';

const report = applyReasonFixture.report ?? (() => { throw new Error('fixture'); })();
const K: [string, string] = ['神奈川県', '横浜市'];
const opts = { months: 6, kind: 'motive' as const, area: 'applicant' as const, occupation: null };

describe('labelsOf', () => {
  it('応募動機と優先順位を分け、文章は数えない', () => {
    const r = row('2026-10-01', K, K, null, { sel: ['給与／手当', '仕事内容'], pri: ['休日／休暇', '給与／手当'], texts: ['給料が良い'] });
    expect(labelsOf(r, 'motive')).toEqual(['給与／手当', '仕事内容']);
    expect(labelsOf(r, 'priority')).toEqual(['休日／休暇', '給与／手当']);
    expect(labelsOf(row('2026-10-01', K, K, null, { texts: ['家から近い'] }), 'motive')).toEqual([]);
  });
  it('①〜③で同じ値は 1 回', () => {
    expect(labelsOf(row('2026-10-01', K, K, null, { pri: ['給与／手当', '給与／手当'] }), 'priority')).toEqual(['給与／手当']);
  });
});

describe('columnsOf', () => {
  it('HubSpot の 13 個の順、選択肢に無い値は後ろに足す', () => {
    const cols = columnsOf(report, 'motive');
    expect(cols.slice(0, 13)).toEqual(REASON_OPTIONS);
    expect(cols).toHaveLength(14);
    expect(cols[13]).toBe('名前が変わった選択肢');
    expect(columnsOf(report, 'priority')).toEqual(REASON_OPTIONS);
  });
});

describe('buildCrosstab', () => {
  it('期間と表ごとに、値がある応募だけを数える', () => {
    expect([1, 3, 6].map(months => buildCrosstab(report, { ...opts, months }).inTable)).toEqual([4, 6, 6]);
    expect([1, 6].map(months => buildCrosstab(report, { ...opts, months, kind: 'priority' }).inTable)).toEqual([3, 3]);
  });
  it('応募動機・応募者の住所: 神奈川県 3 件、給与 2・勤務地 1・休日 1', () => {
    const c = buildCrosstab(report, opts);
    expect(c.area.map(p => [p.label, p.withReason])).toEqual([['神奈川県', 3], ['埼玉県', 1], ['東京都', 1]]);
    expect(c.area[0]?.counts).toMatchObject({ '給与／手当': 2, '勤務地／通勤（車通勤可等）': 1, '休日／休暇': 1, 仕事内容: 0 });
    expect(c.excluded.noArea).toBe(1);
  });
  it('優先順位: ①〜③のどれかに選んだ応募を数える', () => {
    const c = buildCrosstab(report, { ...opts, kind: 'priority' });
    expect(c.area.map(p => [p.label, p.withReason])).toEqual([['神奈川県', 2], ['東京都', 1]]);
    expect(c.area[0]?.counts).toMatchObject({ '給与／手当': 2, '休日／休暇': 2, '勤務地／通勤（車通勤可等）': 1 });
  });
  it('求人の勤務地が分からない応募 (null) は地域別から外す', () => {
    const c = buildCrosstab(report, { ...opts, kind: 'priority', area: 'job' });
    expect(c.excluded.noArea).toBe(1);
    expect(c.area.map(p => [p.label, p.withReason])).toEqual([['神奈川県', 2]]);
  });
  it('職種が無い応募は職種別の「未分類」の行', () => {
    const c = buildCrosstab(report, opts);
    expect(c.excluded).toMatchObject({ noOccupation: 1, occupationConflict: 1 });
    expect(c.occupation.map(o => [o.label, o.withReason])).toEqual([['営業', 1], ['事務', 2], ['物流', 2], ['未分類', 1]]);
  });
  it('最初の記録日を返す (起点より後なら画面に出す)', () => {
    expect(buildCrosstab(report, opts).firstRecorded).toBe('2026-09-18');
    expect(buildCrosstab(report, { ...opts, kind: 'priority' }).firstRecorded).toBe('2026-09-22');
  });
  it('5 件未満の行は割合 null、5 件以上は小数 1 位', () => {
    expect(formatShare(2, 4)).toBeNull();
    expect(formatShare(3, 6)).toBe('50.0%');
    expect(buildCrosstab(report, opts).hiddenShare).toBe(true);
  });
});
