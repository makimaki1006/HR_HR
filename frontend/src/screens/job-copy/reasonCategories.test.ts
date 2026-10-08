import { describe, expect, it } from 'vitest';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { classifyApplicationReasons, classifyTransferReasons, inferCategories, overviewReasonText, reasonsByPeriod, selectedCategory, shareText, tally, topReasons } from './reasonCategories';
import { demoApplicantReasons } from './demoReasons';

const key = (n: number) => String(n).repeat(64);
function collection(overrides: Partial<ApplicantReasonCollection> = {}): ApplicantReasonCollection {
  return { available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-10-08T00:00:00Z', totalApplicants: 3, totalSourceValues: 18,
    sourceCounts: {}, missing: 0, blank: 0, truncated: false, items: [], selections: [], ...overrides };
}
const text = (applicant: number | null, id: string, value: string, sourceProperty = 'oubodouki', applicationDate: string | null = '2026-09-01') =>
  ({ id: id.padEnd(64, '0'), applicant: applicant === null ? null : key(applicant), text: value, sourceProperty, applicationDate, collectedAt: null, versionId: null });
const choice = (applicant: number, value: string, label: string | null, applicationDate: string | null = '2026-09-01', sourceProperty = 'ouboriyuukategori_hiaringu') =>
  ({ applicant: key(applicant), sourceProperty, value, label, applicationDate });

describe('keyword dictionary', () => {
  it.each([
    ['時給が高い', ['給与']],
    ['家から近い', ['勤務地']],
    ['介護の仕事がしたい', ['職種興味']],
    ['大手で安心', ['会社規模']],
    ['土日休みが良い', ['その他']],
    ['月給が良く、駅から近い', ['給与', '勤務地']],
    ['ﾎﾞｰﾅｽがある', ['給与']],
    ['特になし', []],
    ['介護施設', []],
    // Words that mean something else in these phrases are not used.
    ['好きな時間に働ける', []],
    ['介護の仕事が好き', ['職種興味']],
    ['シフトが安定している', ['その他']],
    ['安定した収入が欲しい', ['給与']],
    ['収入が安定する', ['給与']],
    ['経営が安定した会社', ['会社規模']],
    ['近隣の大手スーパーより時給が低い', ['給与']],
    ['大手で安心', ['会社規模']],
    ['定年が近い', []],
    ['理想に近い', []],
    ['家が近いので', ['勤務地']],
  ])('%s → %j', (value, expected) => {
    expect(inferCategories(value)).toEqual(expected);
  });
});

describe('chosen categories', () => {
  it('reads the label first, then the stored value, and knows 未設定', () => {
    expect(selectedCategory({ value: 'kyuuyo', label: '給与' })).toBe('給与');
    expect(selectedCategory({ value: '勤務地', label: null })).toBe('勤務地');
    expect(selectedCategory({ value: 'mise', label: '未設定' })).toBe('unset');
    expect(selectedCategory({ value: 'kyuuyo', label: null })).toBeNull();
  });
});

describe('classification by application', () => {
  it('keeps 選択済み and キーワードで推定 apart and groups the texts and choices of one application', () => {
    const result = classifyApplicationReasons(collection({
      items: [text(1, 'a1', '時給が高い'), text(1, 'a2', '家から近い', 'ouboriyuu_hiaringu'), text(2, 'b1', '家から近い'), text(3, 'c1', '特になし'),
        text(4, 'd1', '時給が高い', 'genshokumaeshokukaranotenshokuriyuu')],
      selections: [choice(1, 'kyuuyo', '給与')],
    }));
    if (!result) throw new Error('not classified');
    expect(result.unit).toBe('application');
    const byKey = Object.fromEntries(result.applications.map(row => [row.key, [row.basis, row.categories]]));
    // Application 1 chose 給与: its texts are not used to add 勤務地.
    expect(byKey[key(1)]).toEqual(['selected', ['給与']]);
    expect(byKey[key(2)]).toEqual(['estimated', ['勤務地']]);
    expect(byKey[key(3)]).toEqual(['unclassified', []]);
    // A transfer reason alone is not an application reason.
    expect(byKey[key(4)]).toBeUndefined();
    const counted = tally(result.applications);
    expect([counted.n, counted.selectedN, counted.estimatedN, counted.unclassified]).toEqual([3, 1, 1, 1]);
    expect(counted.counts.find(row => row.category === '給与')).toEqual({ category: '給与', selected: 1, estimated: 0, total: 1 });
    expect(counted.counts.find(row => row.category === '勤務地')).toEqual({ category: '勤務地', selected: 0, estimated: 1, total: 1 });
  });

  it('treats 未設定 as no choice: the text is used, and an application with only 未設定 is not counted', () => {
    const result = classifyApplicationReasons(collection({
      items: [text(1, 'a1', '時給が高い')],
      selections: [choice(1, 'mise', '未設定'), choice(2, 'mise', '未設定')],
    }));
    expect(result?.applications.map(row => [row.basis, row.categories])).toEqual([['estimated', ['給与']]]);
    expect(result?.unsetOnly).toBe(1);
  });

  it('shows a chosen option name that names no category, but never an internal value without a name', () => {
    const named = classifyApplicationReasons(collection({ selections: [choice(1, 'fukuri', '福利厚生')] }));
    expect(named?.applications[0]).toMatchObject({ basis: 'unclassified', otherValues: ['福利厚生'], unnamedSelections: 0 });
    // The definition read failed (label null) and the value is a code: it is not shown and not 選択済み.
    const unnamed = classifyApplicationReasons(collection({ items: [text(2, 'b1', '時給が高い')], selections: [choice(1, 'kyuuyo', null), choice(2, 'kyuuyo', null)] }));
    expect(unnamed?.applications.map(row => [row.basis, row.categories, row.otherValues, row.unnamedSelections])).toEqual([
      ['estimated', ['給与'], [], 1],
      ['unclassified', [], [], 1],
    ]);
    expect(unnamed?.unnamedApplications).toBe(2);
    expect(tally(unnamed?.applications ?? []).selectedN).toBe(0);
  });

  it('counts each text on its own in a stored file without applicant keys', () => {
    const result = classifyApplicationReasons(collection({ items: [text(null, 'a1', '時給が高い'), text(null, 'b1', '家から近い')], selections: null }));
    expect(result?.unit).toBe('text');
    expect(result?.applications).toHaveLength(2);
  });

  it('classifies transfer reasons on their own, and says 未取得 when the source was not read', () => {
    const read = collection({ sourceCounts: { genshokumaeshokukaranotenshokuriyuu: { missing: 2, blank: 0, nonblank: 1 } }, items: [text(4, 'd1', '給料が安い', 'genshokumaeshokukaranotenshokuriyuu')] });
    expect(classifyTransferReasons(read)?.applications.map(row => row.categories)).toEqual([['給与']]);
    expect(classifyTransferReasons(collection())).toBeNull();
    expect(classifyApplicationReasons(undefined)).toBeNull();
  });
});

describe('shares, periods and the overview', () => {
  it('gives no share below n=5', () => {
    expect(shareText(2, 4)).toBeNull();
    expect(shareText(2, 5)).toBe('40%');
  });

  it('counts per period by application date, with undated and outside counts apart', () => {
    const result = classifyApplicationReasons(collection({ items: [
      text(1, 'a1', '時給が高い', 'oubodouki', '2026-09-01'), text(2, 'b1', '家から近い', 'oubodouki', '2026-09-05'),
      text(3, 'c1', '時給が高い', 'oubodouki', null), text(5, 'e1', '時給が高い', 'oubodouki', '2026-08-01'),
    ] }));
    if (!result) throw new Error('not classified');
    const periods = reasonsByPeriod(result.applications, [{ key: 'v1', start: '2026-09-01', end: '2026-09-05' }, { key: 'between', start: '2026-09-05', end: '2026-09-10' }]);
    expect(periods.periods.map(period => [period.key, period.tally.n, period.tally.counts.filter(row => row.total).map(row => `${row.category}${String(row.total)}`)])).toEqual([
      ['v1', 1, ['給与1']], ['between', 1, ['勤務地1']],
    ]);
    expect([periods.undated, periods.outside]).toEqual([1, 1]);
  });

  it('names the two most frequent reasons with counts and n, or 記録なし / 未取得', () => {
    const demo = demoApplicantReasons();
    // Demo: 給与 = chosen(1) + 時給(7, 未設定) = 2; 勤務地 = 家から近い(2) + chosen(3) = 2; 職種興味 2 (4, 8);
    // 会社規模 1 (9); その他 1 (5); 特になし unclassified (6). n = 9.
    expect(overviewReasonText(demo)).toBe('給与 2件（選択1・推定1）・勤務地 2件（選択1・推定1）／n=9');
    const counted = tally(classifyApplicationReasons(demo)?.applications ?? []);
    expect(topReasons(counted, 3).map(row => [row.category, row.selected, row.estimated])).toEqual([['給与', 1, 1], ['勤務地', 1, 1], ['職種興味', 1, 1]]);
    expect(overviewReasonText(collection())).toBe('記録なし');
    expect(overviewReasonText(undefined)).toBe('未取得');
  });

  it('marks an old stored file in the overview: texts as 記述n, choices 未取得, and no plain 記録なし', () => {
    const old = collection({ selections: null, items: [text(null, 'a1', '時給が高い'), text(null, 'a2', '月給が良い'), text(null, 'b1', '家から近い')] });
    expect(overviewReasonText(old)).toBe('給与 2件（推定2）・勤務地 1件（推定1）／記述n=3（応募ごとではない）（分類の選択は未取得）');
    expect(overviewReasonText(collection({ selections: null }))).toBe('文の記録なし（分類の選択は未取得）');
    expect(overviewReasonText(collection({ truncated: true, items: [text(1, 'a1', '時給が高い')] }))).toBe('給与 1件（推定1）／n=1（記述の一部だけで集計）');
  });
});
