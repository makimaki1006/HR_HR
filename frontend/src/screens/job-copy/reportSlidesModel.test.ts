import { describe, expect, it } from 'vitest';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import type { ApplicationRow } from './data';
import type { LoadedApplications } from './listingApplicationDates';
import { combineApplicants, parseReportTarget } from './reportSlidesLoad';
import {
  applicantsHeadline, buildReport, changesHeadline, clampPeriod, listingsHeadline, periodText, reasonsHeadline, summaryHeadline, trendHeadline,
} from './reportSlidesModel';
import type { ReportInput, ReportListing, ReportOptions, Slide } from './reportSlidesModel';

// 架空の求人・応募（実在しない名前と数）。期間は 2026-10-19（月）〜 11-15（日）の4週、前の期間は 9/21〜10/18。
const truck: ReportListing = {
  id: '101', title: '合成トラックドライバー', media: 'hrh', ended: false, endedOn: null,
  byDate: { '2026-09-25': 1, '2026-10-05': 1, '2026-10-20': 2, '2026-10-28': 1, '2026-11-03': 2 },
  billing: [], billingUnavailable: false,
  events: { listingId: '101', bodyChangedAt: null, bodyChanges: ['2026-10-19'], imageChanges: ['2026-11-01'], startOn: '2026-08-01', statusEvents: [] },
};
const route: ReportListing = {
  id: '102', title: '合成ルート配送', media: 'hrh', ended: true, endedOn: '2026-11-05',
  byDate: { '2026-10-02': 1, '2026-10-25': 2 }, billing: [], billingUnavailable: false,
  events: { listingId: '102', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-09-01', statusEvents: [{ date: '2026-11-05', kind: 'end' }] },
};
const airwork: ReportListing = { id: '103', title: '合成倉庫スタッフ', media: 'airwork', ended: false, endedOn: null, byDate: null, billing: [], billingUnavailable: true, events: null };

const row = (key: string, date: string | null, gender: string, age: string, prefecture: string, municipality: string): ApplicationRow => ({ key, date, gender, age, prefecture, municipality });
const rows: ApplicationRow[] = [
  row('k1', '2026-10-20', '男性', '35', '神奈川県', '横浜市'),
  row('k2', '2026-10-20', '男性', '36', '神奈川県', '横浜市'),
  row('k3', '2026-10-25', '男性', '38', '神奈川県', '横浜市'),
  row('k4', '2026-10-28', '男性', '45', '神奈川県', '横浜市'),
  row('k5', '2026-11-03', '女性', '25', '東京都', '新宿区'),
  row('k6', '2026-11-03', '女性', '62', '東京都', '渋谷区'),
  row('k7', '2026-09-25', '男性', '50', '千葉県', '千葉市'),
  row('k8', null, '男性', '50', '千葉県', '千葉市'),
];
const reason = (applicant: string, text: string, applicationDate: string) => ({ id: `r-${applicant}`, applicant, text, sourceProperty: 'oubodouki', applicationDate, collectedAt: null, versionId: null });
const reasons = (items: ReturnType<typeof reason>[]): ApplicantReasonCollection => ({
  available: true, basis: 'live', fetchedAt: '2026-11-15T00:00:00Z', totalApplicants: items.length, totalSourceValues: items.length,
  sourceCounts: { oubodouki: { missing: 0, blank: 0, nonblank: items.length } }, missing: 0, blank: 0, truncated: false, items, selections: [], optionLabels: 'read',
});
const collection = reasons([
  reason('k1', '給与が良いので', '2026-10-20'),
  reason('k2', '給与と、家から近いので', '2026-10-20'),
  reason('k3', '自宅から近い', '2026-10-25'),
  reason('k4', '給料が高い', '2026-10-28'),
  reason('k7', '給与', '2026-09-25'),
]);

const input = (patch: Partial<ReportInput> = {}): ReportInput => ({
  scope: 'deal', id: '901', name: '合成・定期採用', company: '合成運輸', listings: [truck, route, airwork], overlap: { '2026-10-20': 1 },
  asOf: '2026-11-15', outcomes: { interviews: 4, hires: 1 }, applicants: { rows, reasons: [collection] }, ...patch,
});
const options = (patch: Partial<ReportOptions> = {}): ReportOptions => ({
  from: '2026-10-19', to: '2026-11-15', addressee: '合成運輸', subject: '', owner: '担当A', sender: '', createdOn: '2026-11-15', nextActions: [], nextHeadline: '', ...patch,
});
const slide = <T extends Slide['id']>(slides: Slide[], id: T) => slides.find((item): item is Extract<Slide, { id: T }> => item.id === id);

describe('見出しの1文', () => {
  it('まとめは前の期間と比べた件数だけを書く', () => {
    expect(summaryHeadline(12, 7)).toBe('応募は前の期間の 7件から 12件に増えました');
    expect(summaryHeadline(3, 7)).toBe('応募は前の期間の 7件から 3件に減りました');
    expect(summaryHeadline(5, 5)).toBe('応募は前の期間と同じ 5件でした');
    expect(summaryHeadline(5, null)).toBe('この期間の応募は 5件です');
  });
  it('推移は最も多い週。同じ数の週が複数あれば日付を出さない', () => {
    expect(trendHeadline([{ label: '', count: 1 }, { label: '', count: 3 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('応募が最も多かったのは10月26日の週で、3件です');
    expect(trendHeadline([{ label: '', count: 2 }, { label: '', count: 2 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('期間中の応募は 4件で、週ごとでは最も多いときで 2件です');
    expect(trendHeadline([{ label: '', count: 0 }], 'month', ['2026-10-01'])).toBe('この期間に応募はありませんでした');
    expect(trendHeadline([{ label: '', count: 4 }], 'month', ['2026-10-01'])).toBe('応募が最も多かったのは10月で、4件です');
  });
  it('求人ごとは最も多い求人。並んだら件数だけ', () => {
    expect(listingsHeadline([{ label: 'A', count: 5 }, { label: 'B', count: 2 }])).toBe('応募が最も多かったのは「A」で、5件です');
    expect(listingsHeadline([{ label: 'A', count: 2 }, { label: 'B', count: 2 }])).toBe('応募が最も多かった求人は 2件あり、それぞれ 2件です');
    expect(listingsHeadline([{ label: 'A', count: 0 }])).toBe('この期間は、どの求人にも応募がありませんでした');
  });
  it('前後は最も増えた見直しだけを書き、増えた見直しが無ければ件数の説明だけ', () => {
    const card = { listingId: '1', title: 'A', what: '本文を見直し' };
    expect(changesHeadline([{ ...card, date: '2026-09-15', before: 1, after: 6 }, { ...card, date: '2026-10-13', what: '画像を見直し', before: 3, after: 3 }])).toBe('9月15日の「A」の本文の見直しの前後4週で、応募は 1件から 6件になりました');
    expect(changesHeadline([{ ...card, date: '2026-10-13', before: 4, after: 2 }])).toBe('掲載内容を見直した前後4週の応募件数です');
  });
  it('応募者の傾向は、ひとつに決まるものだけを書く（不明・その他は使わない）', () => {
    const groups = [{ title: '性別', rows: [{ label: '男性', count: 9 }, { label: '女性', count: 3 }] }, { title: 'お住まい', rows: [{ label: '神奈川県', count: 7 }, { label: 'その他', count: 5 }] }];
    expect(applicantsHeadline(groups, 12)).toBe('応募者は男性が 9件で最も多く、お住まいは神奈川県が 7件で最も多くなっています');
    expect(applicantsHeadline([{ title: '性別', rows: [{ label: '男性', count: 2 }, { label: '女性', count: 2 }] }, { title: 'お住まい', rows: [{ label: 'その他', count: 4 }] }], 4)).toBe('応募者 4件の内訳です');
  });
  it('応募理由は「その他」を除いて最も多いもの', () => {
    expect(reasonsHeadline([{ label: '給与・手当', count: 4 }, { label: '勤務地・通勤', count: 2 }])).toBe('うかがえた応募理由は「給与・手当」が最も多くなっています');
    expect(reasonsHeadline([{ label: '給与・手当', count: 2 }, { label: '勤務地・通勤', count: 2 }, { label: 'その他', count: 5 }])).toBe('うかがえた応募理由は「給与・手当」と「勤務地・通勤」が最も多くなっています');
    expect(reasonsHeadline([{ label: 'その他', count: 3 }])).toBe('うかがえた応募理由の内訳です');
  });
});

describe('期間', () => {
  it('終わりを応募データの最後の日までにそろえる', () => {
    expect(clampPeriod('2026-10-01', '2026-11-30', '2026-11-15')).toEqual({ from: '2026-10-01', to: '2026-11-15', clamped: true });
    expect(clampPeriod('2026-12-01', '2026-12-31', '2026-11-15')).toEqual({ from: '2026-11-15', to: '2026-11-15', clamped: true });
    expect(periodText('2026-09-01', '2026-11-15')).toBe('2026年9月1日〜11月15日');
    expect(periodText('2025-12-01', '2026-01-15')).toBe('2025年12月1日〜2026年1月15日');
  });
});

describe('案件の単位のまとめ', () => {
  const result = buildReport(input(), options());
  it('求人をまとめ、重なる応募は1回だけ数え、前の期間と比べる', () => {
    const summary = slide(result.slides, 'summary');
    // 今の期間: 合成トラック 5 + 合成ルート配送 2 − 重なり 1 = 6。前の期間: 2 + 1 = 3。
    expect(summary?.headline).toBe('応募は前の期間の 3件から 6件に増えました');
    expect(summary?.kpis).toEqual([
      { label: '応募', value: '6件', sub: '前の期間 3件' },
      { label: '面接', value: '4件', sub: 'ご契約からの合計' },
      { label: '採用', value: '1名', sub: 'ご契約からの合計' },
      { label: '掲載中の求人', value: '2件', sub: '求人 3件のうち' },
    ]);
    expect(summary?.points).toEqual(['10月19日の「合成トラックドライバー」の本文の見直しの前後4週で、応募は 2件から 5件になりました', 'うかがえた応募理由は「給与・手当」が最も多くなっています']);
  });
  it('週ごとの応募と見直しの印', () => {
    const trend = slide(result.slides, 'trend');
    expect(trend?.unit).toBe('week');
    expect(trend?.bars).toEqual([
      { label: '10/19', count: 3, marks: ['本文を見直し'] },
      { label: '10/26', count: 1, marks: ['画像を見直し'] },
      { label: '11/2', count: 2, marks: [] },
      { label: '11/9', count: 0, marks: [] },
    ]);
    expect(trend?.headline).toBe('応募が最も多かったのは10月19日の週で、3件です');
  });
  it('求人ごとの応募は多い順。終了した求人には終了日', () => {
    const listings = slide(result.slides, 'listings');
    expect(listings?.bars).toEqual([{ label: '合成トラックドライバー', count: 5 }, { label: '合成ルート配送（11/5 終了）', count: 2 }]);
    expect(listings?.headline).toBe('応募が最も多かったのは「合成トラックドライバー」で、5件です');
  });
  it('変えたことと前後は、前後4週がそろった見直しだけ', () => {
    expect(slide(result.slides, 'changes')?.cards).toEqual([{ date: '2026-10-19', listingId: '101', title: '合成トラックドライバー', what: '本文を見直し', before: 2, after: 5 }]);
  });
  it('応募者の傾向は期間内の応募日の分。3件未満の地域は「その他」', () => {
    const applicants = slide(result.slides, 'applicants');
    expect(applicants?.total).toBe(6);
    expect(applicants?.groups).toEqual([
      { title: '性別', rows: [{ label: '男性', count: 4 }, { label: '女性', count: 2 }] },
      { title: '年代', rows: [{ label: '20代', count: 1 }, { label: '30代', count: 3 }, { label: '40代', count: 1 }, { label: '60歳以上', count: 1 }] },
      { title: 'お住まい', rows: [{ label: '神奈川県', count: 3 }, { label: 'その他', count: 3 }] },
    ]);
    expect(applicants?.headline).toBe('応募者は男性が 4件で最も多く、お住まいは神奈川県が 3件で最も多くなっています');
    expect(applicants?.note).toBe('期間中の応募 6件の内訳（件）。3件未満の地域は「その他」にまとめています。');
  });
  it('応募理由は期間内の応募の分（複数回答）', () => {
    const reasonsSlide = slide(result.slides, 'reasons');
    expect(reasonsSlide?.n).toBe(4);
    expect(reasonsSlide?.bars).toEqual([{ label: '給与・手当', count: 3 }, { label: '勤務地・通勤', count: 2 }]);
  });
  it('分からない数字は紙に書かず、外した理由を返す', () => {
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'summary', 'trend', 'listings', 'changes', 'applicants', 'reasons']);
    expect(result.excluded).toEqual([
      { name: '応募の件数', reason: '応募日を読み込めなかった求人 1件（合成倉庫スタッフ）は件数に入っていません' },
      { name: 'まとめ（費用）', reason: 'この期間の課金の実績が無い、または一部の求人で分からないため外しています' },
      { name: '変えたことと前後の応募', reason: '前後4週の応募データがそろっていない見直し 1件は外しています' },
      { name: '次に行うこと', reason: '「次に行うこと」が未記入のため' },
    ]);
    expect(JSON.stringify(result.slides)).not.toMatch(/未取得|HubSpot|pt|効果/);
  });
  it('表紙は宛名・件名・期間・担当', () => {
    expect(slide(result.slides, 'cover')).toMatchObject({ addressee: '合成運輸 御中', title: '採用活動のご報告', subtitle: '合成・定期採用 ／ 2026年10月19日〜11月15日', footer: '担当：担当A ／ 2026年11月15日' });
  });
  it('次に行うことは3項目まで。書けば8枚そろう', () => {
    const full = buildReport(input(), options({ nextActions: [{ text: '給与の書き方をほかの求人にも広げます', sub: '11月下旬まで' }, { text: '', sub: '' }, { text: '応募理由を引き続きうかがいます', sub: '' }, { text: '4つ目', sub: '' }] }));
    expect(full.slides).toHaveLength(8);
    expect(slide(full.slides, 'next')).toMatchObject({ headline: '給与の書き方をほかの求人にも広げます', items: [{ text: '給与の書き方をほかの求人にも広げます', sub: '11月下旬まで' }, { text: '応募理由を引き続きうかがいます', sub: '' }] });
    expect(slide(full.slides, 'summary')?.points.at(-1)).toBe('次は、給与の書き方をほかの求人にも広げます');
  });
  it('課金がそろう期間だけ費用を入れる', () => {
    const billing = [{ source: 'hrhacker' as const, start: '2026-10-19', end: '2026-11-15', amountYen: 28000 }];
    const priced = buildReport(input({ listings: [{ ...truck, billing }, { ...route, billing: [{ source: 'hrhacker', start: '2026-10-19', end: '2026-11-15', amountYen: 0 }] }] }), options());
    expect(slide(priced.slides, 'summary')?.kpis.at(-1)).toEqual({ label: '費用', value: '2万8,000円', sub: '応募1件あたり 4,667円' });
  });
});

describe('外す判定', () => {
  it('応募日がどの求人も分からなければ、件数を使うスライドを外す', () => {
    const result = buildReport(input({ listings: [airwork], applicants: { rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' } }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover']);
    expect(result.excluded.map(item => `${item.name}：${item.reason}`)).toEqual([
      'まとめ：応募日別の件数を読み込めなかったため', '応募の推移：応募日別の件数を読み込めなかったため', '求人ごとの応募：応募日別の件数を読み込めなかったため', '変えたことと前後の応募：応募日別の件数を読み込めなかったため',
      '応募者の傾向：一部の求人の応募者を読み込めなかったため', '応募理由：一部の求人の応募者を読み込めなかったため', '次に行うこと：「次に行うこと」が未記入のため',
    ]);
  });
  it('求人の単位では「求人ごとの応募」と面接・採用を出さない', () => {
    const result = buildReport(input({ scope: 'listing', listings: [truck], overlap: {}, outcomes: null }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'summary', 'trend', 'changes', 'applicants', 'reasons']);
    expect(result.excluded).toContainEqual({ name: '求人ごとの応募', reason: '求人1件の報告のため' });
    expect(slide(result.slides, 'summary')?.kpis.map(kpi => kpi.label)).toEqual(['応募', '掲載中の求人']);
  });
  it('前の期間の初めより前のデータが無ければ比べない', () => {
    const young = { ...truck, events: { ...truck.events, listingId: '101', bodyChangedAt: null, startOn: '2026-10-01' }, byDate: { '2026-10-05': 1, '2026-10-20': 2 } };
    const result = buildReport(input({ listings: [young], overlap: {} }), options());
    expect(slide(result.slides, 'summary')?.headline).toBe('この期間の応募は 2件です');
    expect(result.excluded).toContainEqual({ name: 'まとめ（前の期間）', reason: '前の期間の初めより前の応募データが無いため、前の期間と比べていません' });
  });
  it('期間内に見直しが無い・前後4週がそろわなければ「変えたことと前後」を外す', () => {
    const none = buildReport(input(), options({ from: '2026-11-02', to: '2026-11-15' }));
    expect(slide(none.slides, 'changes')).toBeUndefined();
    expect(none.excluded).toContainEqual({ name: '変えたことと前後の応募', reason: '期間中に本文・画像の見直しが無いため' });
    // 11/1 の画像の見直しは、後の4週が 11/15 までにそろわない。
    const incomplete = buildReport(input(), options({ from: '2026-10-26', to: '2026-11-15' }));
    expect(slide(incomplete.slides, 'changes')).toBeUndefined();
    expect(incomplete.excluded).toContainEqual({ name: '変えたことと前後の応募', reason: '期間中の見直しは、どれも前後4週の応募データがそろっていないため' });
  });
});

describe('応募者のまとめ方（案件の単位）', () => {
  const loaded = (keys: string[], total = keys.length): LoadedApplications => ({
    overall: { total, missingDate: 0, fetchedAt: '2026-11-15T00:00:00Z', distributions: {}, byDate: {} },
    rows: keys.map(key => row(key, '2026-10-20', '男性', '30', '神奈川県', '横浜市')),
    reasons: reasons([]),
  });
  it('複数の求人に応募した方は1回だけ数える', () => {
    const combined = combineApplicants([loaded(['a', 'b']), loaded(['b', 'c'])], 0);
    expect(combined.rows?.map(item => item.key)).toEqual(['a', 'b', 'c']);
    expect(combined.reasons).toHaveLength(2);
  });
  it('一部の求人だけ読めたとき・属性がそろわないとき・求人が多いときは作らない', () => {
    expect(combineApplicants([loaded(['a']), null], 0)).toEqual({ rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' });
    expect(combineApplicants([loaded(['a'], 2)], 0).rows).toBeNull();
    expect(combineApplicants([loaded(['a'])], 3).missingReason).toBe('求人が多いため（20件まで）、応募者の内訳を作っていません');
  });
  it('URL の report は deal / listing と数字の ID だけ', () => {
    expect(parseReportTarget('deal:123')).toEqual({ scope: 'deal', id: '123' });
    expect(parseReportTarget('listing:45')).toEqual({ scope: 'listing', id: '45' });
    expect(parseReportTarget('company:1')).toBeNull();
    expect(parseReportTarget('deal:abc')).toBeNull();
    expect(parseReportTarget(null)).toBeNull();
  });
});
