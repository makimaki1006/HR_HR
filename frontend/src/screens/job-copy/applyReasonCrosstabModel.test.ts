import { describe, expect, it } from 'vitest';
import { applyReasonFixture, row } from './applyReason.testFixtures';
import { buildCrosstab, classifyRow, formatShare, UNCLASSIFIED } from './applyReasonCrosstabModel';

const report = applyReasonFixture.report ?? (() => { throw new Error('fixture'); })();
const K: [string, string] = ['神奈川県', '横浜市'];

describe('classifyRow', () => {
  it('選択が文より優先される', () => {
    expect(classifyRow(row('2026-10-01', K, K, null, { sel: ['給与'], texts: ['家から近い'] }))).toEqual({ categories: ['給与'], basis: 'selected' });
  });
  it('未設定の選択は無視して文で推定する', () => {
    expect(classifyRow(row('2026-10-01', K, K, null, { sel: ['未設定'], texts: ['駅から近い'] }))).toEqual({ categories: ['勤務地'], basis: 'estimated' });
  });
  it('文は結合して複数の区分に入る', () => {
    expect(classifyRow(row('2026-10-01', K, K, null, { texts: ['給料が良い', '家から近い'] })).categories).toEqual(['給与', '勤務地']);
  });
  it('どれにも当たらなければ分類できない', () => {
    expect(classifyRow(row('2026-10-01', K, K, null, { texts: ['特にありません'] }))).toEqual({ categories: [], basis: 'unclassified' });
  });
});

describe('buildCrosstab', () => {
  it('期間で絞る (1・3・6 か月)', () => {
    expect([1, 3, 6].map(months => buildCrosstab(report, { months, area: 'applicant', occupation: null }).inPeriod)).toEqual([4, 7, 11]);
  });
  it('応募者の住所: 神奈川県 6 件、給与 3・勤務地 3・その他 1', () => {
    const c = buildCrosstab(report, { months: 6, area: 'applicant', occupation: null });
    expect(c.area.map(p => [p.label, p.withReason])).toEqual([['神奈川県', 6], ['埼玉県', 2], ['東京都', 2]]);
    expect(c.area[0]?.counts).toEqual({ 給与: 3, 勤務地: 3, 職種興味: 0, 会社規模: 0, その他: 1, [UNCLASSIFIED]: 0 });
    expect(c.area[0]?.children.map(m => [m.label, m.withReason])).toEqual([['横浜市', 6]]);
  });
  it('求人の勤務地に切り替えると 1 行目の数が変わる', () => {
    const c = buildCrosstab(report, { months: 6, area: 'job', occupation: null });
    expect(c.area[0]).toMatchObject({ label: '神奈川県', withReason: 7 });
    expect(c.area[0]?.children.map(m => [m.label, m.withReason])).toEqual([['横浜市', 4], ['平塚市', 3]]);
  });
  it('地域が null の応募は外した件数に入り、表に出ない', () => {
    const c = buildCrosstab(report, { months: 6, area: 'applicant', occupation: null });
    expect(c.excluded.noArea).toBe(1);
    expect(c.area.reduce((n, p) => n + p.withReason, 0)).toBe(10);
    expect(buildCrosstab(report, { months: 6, area: 'job', occupation: null }).excluded.noArea).toBe(1);
  });
  it('職種 null は理由別に数え、職種別の表では未分類の行に入る', () => {
    const c = buildCrosstab(report, { months: 6, area: 'applicant', occupation: null });
    expect(c.excluded).toMatchObject({ noOccupation: 2, occupationMissing: 0, occupationUnmatched: 1, occupationConflict: 1, unclassified: 1 });
    expect(c.occupation.map(o => [o.label, o.withReason])).toEqual([['営業', 2], ['事務', 2], ['物流', 5], ['未分類', 2]]);
  });
  it('職種の絞り込みは地域別だけに効く', () => {
    const c = buildCrosstab(report, { months: 6, area: 'applicant', occupation: '物流' });
    expect(c.area.map(p => [p.label, p.withReason])).toEqual([['神奈川県', 5]]);
    expect(c.occupation).toHaveLength(4);
  });
  it('5 件未満の行は割合 null、5 件以上は小数 1 位', () => {
    expect(formatShare(2, 4)).toBeNull();
    expect(formatShare(3, 6)).toBe('50.0%');
    const c = buildCrosstab(report, { months: 6, area: 'applicant', occupation: null });
    expect(c.hiddenShare).toBe(true);
  });
});
