import { describe, expect, it } from 'vitest';
import type { ApplicantReasonCollection, ReasonSelection } from './applicantReasonsModel';
import type { ApplicationRow } from './data';
import type { LoadedApplications } from './listingApplicationDates';
import { combineApplicants, parseReportTarget } from './reportSlidesLoad';
import {
  alignedPeriod, applicantGroups, applicantsHeadline, billingCovers, buildReport, changesHeadline, clampPeriod, listingsHeadline, normalizeGender, periodText, reasonBars,
  reasonsHeadline, summaryHeadline, trendHeadline,
} from './reportSlidesModel';
import type { ReportInput, ReportListing, ReportOptions, Slide } from './reportSlidesModel';

// 架空の求人・応募（実在しない名前と数）。期間は 2026-10-19（月）〜 11-15（日）の4週、前の期間は 9/21〜10/18。
// 応募のキーはサーバーと同じく求人ごとに別。同じ方が2つの求人に応募した分（t6 と r3）は、それぞれの求人の multiListingApplicants に入る。
const row = (key: string, date: string, gender: string, age: string, prefecture = '神奈川県', municipality = '横浜市'): ApplicationRow => ({ key, date, gender, age, prefecture, municipality });
const truckRows = [
  row('t0', '2026-09-10', '男', '52', '千葉県', '千葉市'), row('t1', '2026-09-25', '男', '50', '千葉県', '千葉市'),
  row('t2', '2026-10-20', '男', '35'), row('t3', '2026-10-20', '男', '36'), row('t6', '2026-10-25', '男', '33'),
  row('t4', '2026-10-28', '男性', '45'), row('t5', '2026-11-03', '女', '25', '東京都', '新宿区'),
];
const routeRows = [row('r1', '2026-10-02', '男', '44'), row('r2', '2026-10-25', '女性', '29'), row('r3', '2026-10-25', '男', '33')];
const byDateOf = (rows: ApplicationRow[]) => rows.reduce<Record<string, number>>((all, item) => ({ ...all, [item.date ?? '']: (all[item.date ?? ''] ?? 0) + 1 }), {});
const pick = (applicant: string, value: string, applicationDate: string, sourceProperty = 'ouboriyuukategori_hiaringu'): ReasonSelection => ({ applicant, sourceProperty, value, label: value, applicationDate });
const collection = (selections: ReasonSelection[], multi: string[] | null, truncated = false): ApplicantReasonCollection => ({
  available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-11-15T00:00:00Z', totalApplicants: 0, totalSourceValues: 0, sourceCounts: {}, missing: 0, blank: 0, truncated,
  items: [{ id: 'x', applicant: 'r2', text: '家から近い', sourceProperty: 'ouboriyuu_hiaringu', applicationDate: '2026-10-25', collectedAt: null, versionId: null }],
  selections, optionLabels: 'read', multiListingApplicants: multi,
});
const loaded = (rows: ApplicationRow[], reasons: ApplicantReasonCollection): LoadedApplications => ({ overall: { total: rows.length, missingDate: 0, fetchedAt: '2026-11-15T00:00:00Z', distributions: {}, byDate: byDateOf(rows) }, rows, reasons });
const truckReasons = collection([
  pick('t1', '給与', '2026-09-25'), pick('t2', '給与', '2026-10-20'), pick('t3', '給与', '2026-10-20'), pick('t3', '勤務地', '2026-10-20'),
  pick('t6', '給与', '2026-10-25'), pick('t4', '勤務地', '2026-10-28'), pick('t4', '会社規模', '2026-10-28', 'ouboriyuukategori_baitaikisai'), pick('t5', '給与', '2026-11-03'),
], ['t6']);
const routeReasons = collection([pick('r2', '職種興味', '2026-10-25'), pick('r3', '給与', '2026-10-25')], ['r3']);
const applicants = combineApplicants([loaded(truckRows, truckReasons), loaded(routeRows, routeReasons)], 0, 'deal');

const truck: ReportListing = {
  id: '101', title: '合成トラックドライバー', media: 'hrh', ended: false, endedOn: null, byDate: byDateOf(truckRows), billing: [], billingUnavailable: false,
  events: { listingId: '101', bodyChangedAt: null, bodyChanges: ['2026-10-19'], imageChanges: ['2026-11-01'], startOn: '2026-08-01', statusEvents: [] },
};
const route: ReportListing = {
  id: '102', title: '合成ルート配送', media: 'hrh', ended: true, endedOn: '2026-11-05', byDate: byDateOf(routeRows), billing: [], billingUnavailable: false,
  events: { listingId: '102', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-09-01', statusEvents: [{ date: '2026-11-05', kind: 'end' }] },
};
const input = (patch: Partial<ReportInput> = {}): ReportInput => ({
  scope: 'deal', id: '901', name: '合成・定期採用', company: '合成運輸', listings: [truck, route], overlap: { '2026-10-25': 1 },
  asOf: '2026-11-15', outcomes: { interviews: 4, hires: 1 }, applicants, ...patch,
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
  it('推移は最も多い週。端数の週は見出しの対象にせず、並んだら日付を出さない', () => {
    expect(trendHeadline([{ label: '', count: 1 }, { label: '', count: 3 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('応募が最も多かったのは10月26日の週で、3件です');
    expect(trendHeadline([{ label: '', count: 5 }, { label: '', count: 3 }], 'week', ['2026-10-22', '2026-10-26'], [false, true])).toBe('応募が最も多かったのは10月26日の週で、3件です');
    expect(trendHeadline([{ label: '', count: 2 }, { label: '', count: 2 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('期間中の応募は 4件で、週ごとでは最も多いときで 2件です');
    expect(trendHeadline([{ label: '', count: 0 }], 'month', ['2026-10-01'])).toBe('この期間に応募はありませんでした');
  });
  it('求人ごとは最も多い求人。長い求人名は20字で省く', () => {
    expect(listingsHeadline([{ label: 'A', count: 5 }, { label: 'B', count: 2 }])).toBe('応募が最も多かったのは「A」で、5件です');
    expect(listingsHeadline([{ label: 'あ'.repeat(30), count: 5 }])).toBe(`応募が最も多かったのは「${'あ'.repeat(20)}…」で、5件です`);
    expect(listingsHeadline([{ label: 'A', count: 2 }, { label: 'B', count: 2 }])).toBe('応募が最も多かった求人は 2件あり、それぞれ 2件です');
  });
  it('前後は最も増えた見直しだけを書き、増えた見直しが無ければ件数の説明だけ', () => {
    const card = { listingId: '1', title: 'A', what: '本文を見直し' };
    expect(changesHeadline([{ ...card, date: '2026-09-15', before: 1, after: 6 }, { ...card, date: '2026-10-13', what: '画像を見直し', before: 3, after: 3 }])).toBe('9月15日の「A」の本文の見直しの前後4週で、応募は 1件から 6件になりました');
    expect(changesHeadline([{ ...card, date: '2026-10-13', before: 4, after: 2 }])).toBe('掲載内容を見直した前後4週の応募件数です');
  });
  it('応募者の傾向は、ひとつに決まるものだけを書く（不明・その他は使わない）', () => {
    expect(applicantsHeadline([{ title: '性別', rows: [{ label: '男性', count: 2 }, { label: '女性', count: 2 }] }, { title: 'お住まい', rows: [{ label: 'その他', count: 4 }] }], 4)).toBe('応募者 4件の内訳です');
  });
  it('応募理由は「その他」を除いて最も多いもの', () => {
    expect(reasonsHeadline([{ label: '給与・手当', count: 4 }, { label: '勤務地・通勤', count: 2 }])).toBe('お電話でうかがえた応募理由は「給与・手当」が最も多くなっています');
    expect(reasonsHeadline([{ label: '給与・手当', count: 2 }, { label: '勤務地・通勤', count: 2 }, { label: 'その他', count: 5 }])).toBe('お電話でうかがえた応募理由は「給与・手当」と「勤務地・通勤」が最も多くなっています');
    expect(reasonsHeadline([{ label: 'その他', count: 3 }])).toBe('お電話でうかがえた応募理由の内訳です');
  });
});

describe('期間', () => {
  it('終わりを応募データの最後の日までにそろえる', () => {
    expect(clampPeriod('2026-10-01', '2026-11-30', '2026-11-15')).toEqual({ from: '2026-10-01', to: '2026-11-15', clamped: true });
    expect(periodText('2026-09-01', '2026-11-15')).toBe('2026年9月1日〜11月15日');
    expect(periodText('2025-12-01', '2026-01-15')).toBe('2025年12月1日〜2026年1月15日');
  });
  it('既定の期間は月曜始まり〜日曜終わりの丸い週', () => {
    expect(alignedPeriod('2026-11-15', 4)).toEqual({ from: '2026-10-19', to: '2026-11-15' }); // 11/15 は日曜
    expect(alignedPeriod('2026-11-18', 4)).toEqual({ from: '2026-10-19', to: '2026-11-15' }); // 水曜なら前の日曜まで
  });
});

describe('案件の単位のまとめ', () => {
  const result = buildReport(input(), options());
  it('求人をまとめ、重なる応募は1回だけ数え、前の期間と比べる（面接・採用・費用は既定で出さない）', () => {
    const summary = slide(result.slides, 'summary');
    // 今の期間: 合成トラック 5 + 合成ルート配送 2 − 重なり 1 = 6。前の期間: 1 + 1 = 2。
    expect(summary?.headline).toBe('応募は前の期間の 2件から 6件に増えました');
    expect(summary?.kpis).toEqual([
      { label: '応募', value: '6件', sub: '前の期間 2件' },
      { label: '応募のあった求人', value: '2件', sub: '求人 2件のうち' },
      { label: '掲載中の求人', value: '1件', sub: null },
    ]);
    expect(summary?.points).toEqual(['10月19日の「合成トラックドライバー」の本文の見直しの前後4週で、応募は 1件から 5件になりました', 'お電話でうかがえた応募理由は「給与・手当」が最も多くなっています']);
  });
  it('入れないスライドの文はまとめの箇条書きにも入れない', () => {
    const off = buildReport(input(), options({ off: ['changes'] }));
    expect(slide(off.slides, 'summary')?.points).toEqual(['お電話でうかがえた応募理由は「給与・手当」が最も多くなっています']);
  });
  it('面接・採用は入れる指定のときだけ', () => {
    const withOutcomes = buildReport(input(), options({ include: { outcomes: true } }));
    expect(slide(withOutcomes.slides, 'summary')?.kpis.map(kpi => `${kpi.label} ${kpi.value}`)).toEqual(['応募 6件', '面接 4件', '採用 1名', '応募のあった求人 2件', '掲載中の求人 1件']);
  });
  it('週ごとの応募と見直しの印', () => {
    const trend = slide(result.slides, 'trend');
    expect(trend?.bars).toEqual([
      { label: '10/19', count: 4, marks: ['本文を見直し'] },
      { label: '10/26', count: 1, marks: ['画像を見直し'] },
      { label: '11/2', count: 1, marks: [] },
      { label: '11/9', count: 0, marks: [] },
    ]);
    expect(trend?.headline).toBe('応募が最も多かったのは10月19日の週で、4件です');
  });
  it('求人ごとの応募は多い順。終了した求人には終了日', () => {
    expect(slide(result.slides, 'listings')?.bars).toEqual([{ label: '合成トラックドライバー', count: 5 }, { label: '合成ルート配送（11/5 終了）', count: 2 }]);
  });
  it('同じ求人名は媒体と勤務地で区別する', () => {
    const twin = buildReport(input({ listings: [{ ...truck, place: '神奈川県横浜市' }, { ...route, title: '合成トラックドライバー', ended: false, place: '東京都新宿区', media: 'airwork' }] }), options());
    expect(slide(twin.slides, 'listings')?.bars.map(bar => bar.label)).toEqual(['合成トラックドライバー［HRハッカー・神奈川県横浜市］', '合成トラックドライバー［AirWork・東京都新宿区］']);
  });
  it('変えたことと前後は、前後4週がそろった見直しだけ', () => {
    expect(slide(result.slides, 'changes')?.cards).toEqual([{ date: '2026-10-19', listingId: '101', title: '合成トラックドライバー', what: '本文を見直し', before: 1, after: 5 }]);
  });
  it('応募者の傾向: 2つの求人に結び付いた応募は外し、性別の表記ゆれをまとめ、都道府県は単独の内訳', () => {
    expect(applicants.excludedMulti).toBe(2);
    const groups = slide(result.slides, 'applicants');
    expect(groups?.total).toBe(5);
    expect(groups?.groups).toEqual([
      { title: '性別', rows: [{ label: '男性', count: 3 }, { label: '女性', count: 2 }] },
      { title: '年代', rows: [{ label: '20代', count: 2 }, { label: '30代', count: 2 }, { label: '40代', count: 1 }] },
      { title: 'お住まい', rows: [{ label: '神奈川県', count: 4 }, { label: 'その他', count: 1 }] },
    ]);
    // 見出しの件数は内訳の棒と同じ
    expect(groups?.headline).toBe('応募者は男性が 3件で最も多く、お住まいは神奈川県が 4件で最も多くなっています');
    expect(result.excluded).toContainEqual({ name: '応募者の傾向・応募理由', reason: '複数の求人に結び付いた応募 2件（求人ごとに数えた数）は、同じ方を重ねて数えないよう外しています' });
  });
  it('応募理由は電話でうかがって選んだ分類だけ（推定・媒体記載は使わず、複数の求人の応募は外す）', () => {
    const reasons = slide(result.slides, 'reasons');
    expect(reasons?.n).toBe(5);
    expect(reasons?.bars).toEqual([{ label: '給与・手当', count: 3 }, { label: '勤務地・通勤', count: 2 }, { label: '仕事内容への関心', count: 1 }]);
    expect(reasons?.note).toBe('お電話で応募理由をうかがえた 5件の内訳。1件で複数の理由をうかがった場合は、それぞれに数えています。');
  });
  it('紙に社内の言葉・判定の言葉を出さない', () => {
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'summary', 'trend', 'listings', 'changes', 'applicants', 'reasons']);
    expect(JSON.stringify(result.slides)).not.toMatch(/未取得|HubSpot|pt|効果|合成・定期採用/);
  });
  it('表紙の件名は既定で空（取引名を出さない）。空なら期間だけ', () => {
    expect(slide(result.slides, 'cover')).toMatchObject({ addressee: '合成運輸 御中', subtitle: '2026年10月19日〜11月15日', footer: '担当：担当A ／ 2026年11月15日' });
    expect(slide(buildReport(input(), options({ subject: '第2支店' })).slides, 'cover')?.subtitle).toBe('第2支店 ／ 2026年10月19日〜11月15日');
  });
  it('次に行うことは3項目まで。書けば8枚そろう', () => {
    const full = buildReport(input(), options({ nextActions: [{ text: '給与の書き方をほかの求人にも広げます', sub: '11月下旬まで' }, { text: '', sub: '' }, { text: '応募理由を引き続きうかがいます', sub: '' }, { text: '4つ目', sub: '' }] }));
    expect(full.slides).toHaveLength(8);
    expect(slide(full.slides, 'next')?.items).toEqual([{ text: '給与の書き方をほかの求人にも広げます', sub: '11月下旬まで' }, { text: '応募理由を引き続きうかがいます', sub: '' }]);
  });
  it('費用は入れる指定があり、課金の行が期間を切れ目なく覆うときだけ', () => {
    const week = (start: string, end: string, amountYen: number | null) => ({ source: 'hrhacker' as const, start, end, amountYen });
    const full = [week('2026-10-19', '2026-10-25', 7000), week('2026-10-26', '2026-11-01', 7000), week('2026-11-02', '2026-11-08', 7000), week('2026-11-09', '2026-11-15', 7000)];
    expect(billingCovers(full, '2026-10-19', '2026-11-16')).toBe(true);
    expect(billingCovers([full[0], full[2], full[3]].filter(Boolean) as typeof full, '2026-10-19', '2026-11-16')).toBe(false);
    expect(billingCovers([...full.slice(0, 3), week('2026-11-09', '2026-11-15', null)], '2026-10-19', '2026-11-16')).toBe(false);
    const priced = input({ listings: [{ ...truck, billing: full }, { ...route, billing: full.map(item => ({ ...item, amountYen: 0 })) }] });
    expect(slide(buildReport(priced, options()).slides, 'summary')?.kpis.some(kpi => kpi.label === '費用')).toBe(false);
    expect(slide(buildReport(priced, options({ include: { cost: true } })).slides, 'summary')?.kpis.at(-1)).toEqual({ label: '費用', value: '2万8,000円', sub: '応募1件あたり 4,667円' });
  });
});

describe('外す判定', () => {
  it('一部の求人しか数えられないときは、まとめ・推移・求人ごとを外す', () => {
    const airwork: ReportListing = { id: '103', title: '合成倉庫スタッフ', media: 'airwork', ended: false, endedOn: null, byDate: null, billing: [], billingUnavailable: true, events: null };
    const result = buildReport(input({ listings: [truck, route, airwork] }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'changes', 'applicants', 'reasons']);
    expect(result.excluded).toContainEqual({ name: 'まとめ', reason: '求人の一部しか数えられないため（応募日を読み込めなかった求人が 1件あります（合成倉庫スタッフ））' });
    const capped = buildReport(input({ partial: ['求人が多いため、先頭の50件までしか読んでいません'] }), options());
    expect(capped.slides.map(item => item.id)).not.toContain('trend');
  });
  it('応募日がどの求人も分からなければ、件数を使うスライドを外す', () => {
    const none = { ...truck, byDate: null };
    const result = buildReport(input({ listings: [none], applicants: { rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' } }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover']);
  });
  it('求人の単位では「求人ごとの応募」を出さない', () => {
    const result = buildReport(input({ scope: 'listing', listings: [truck], overlap: {}, outcomes: null, applicants: combineApplicants([loaded(truckRows, truckReasons)], 0, 'listing') }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'summary', 'trend', 'changes', 'applicants', 'reasons']);
    expect(slide(result.slides, 'summary')?.kpis.map(kpi => kpi.label)).toEqual(['応募', '掲載中の求人']);
    // 求人の単位でも、ほかの求人に結び付いた応募の理由は外す（求人の画面と同じ）
    expect(slide(result.slides, 'reasons')?.bars).toEqual([{ label: '給与・手当', count: 3 }, { label: '勤務地・通勤', count: 2 }]);
  });
  it('最初の応募が前の期間の初めより後なら比べない', () => {
    const young = { ...truck, byDate: { '2026-10-05': 1, '2026-10-20': 2 } };
    const result = buildReport(input({ listings: [young], overlap: {} }), options());
    expect(slide(result.slides, 'summary')?.headline).toBe('この期間の応募は 2件です');
  });
  it('期間内に見直しが無い・前後4週がそろわなければ「変えたことと前後」を外す', () => {
    expect(buildReport(input(), options({ from: '2026-11-02', to: '2026-11-15' })).excluded).toContainEqual({ name: '変えたことと前後の応募', reason: '期間中に本文・画像の見直しが無いため' });
    expect(buildReport(input(), options({ from: '2026-10-26', to: '2026-11-15' })).excluded).toContainEqual({ name: '変えたことと前後の応募', reason: '期間中の見直しは、どれも前後4週の応募データがそろっていないため' });
  });
  it('応募理由が途中までしか読めていなければ外す', () => {
    expect(reasonBars([collection([], [], true)], '2026-10-19', '2026-11-15')).toEqual({ ok: false, reason: '応募理由の記録が多く、一部しか読み込めていないため' });
  });
});

describe('応募者のまとめ方（案件の単位）', () => {
  it('複数の求人に結び付いた応募を見分けられないときは作らない', () => {
    const unknown = combineApplicants([loaded(truckRows, collection([], null)), loaded(routeRows, routeReasons)], 0, 'deal');
    expect(unknown).toEqual({ rows: null, reasons: null, missingReason: '複数の求人に結び付いた応募を見分けられないため（重ねて数えないよう外しています）' });
  });
  it('一部の求人だけ読めたとき・属性がそろわないとき・求人が多いときは作らない', () => {
    expect(combineApplicants([loaded(truckRows, truckReasons), null], 0)).toEqual({ rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' });
    expect(combineApplicants([{ ...loaded(truckRows, truckReasons), overall: { ...loaded(truckRows, truckReasons).overall, total: 99 } }], 0, 'listing').rows).toBeNull();
    expect(combineApplicants([], 21).missingReason).toBe('応募のある求人が多いため（20件まで）、応募者の内訳を作っていません');
  });
  it('性別の表記ゆれと、3件未満の都道府県', () => {
    expect(['男', '男性', '女', '女性', ' 男 ', null].map(normalizeGender)).toEqual(['男性', '男性', '女性', '女性', '男性', null]);
    expect(applicantGroups([row('a', '2026-10-20', '男', '30'), row('b', '2026-10-20', '男', '30', '東京都', '新宿区')])[2]).toEqual({ title: 'お住まい', rows: [{ label: 'その他', count: 2 }] });
  });
  it('URL の report は deal / listing と数字の ID だけ', () => {
    expect(parseReportTarget('deal:123')).toEqual({ scope: 'deal', id: '123' });
    expect(parseReportTarget('listing:45')).toEqual({ scope: 'listing', id: '45' });
    expect(parseReportTarget('company:1')).toBeNull();
    expect(parseReportTarget('deal:abc')).toBeNull();
  });
});
