import { describe, expect, it } from 'vitest';
import type { ApplicantReasonCollection, ReasonSelection } from './applicantReasonsModel';
import type { ApplicationRow } from './data';
import type { LoadedApplications } from './listingApplicationDates';
import { combineApplicants, parseReportTarget } from './reportSlidesLoad';
import {
  alignedPeriod, applicantGroups, applicantsHeadline, buildReport, changesHeadline, clampPeriod, emptyTexts, normalizeGender, periodText, reasonBars,
  reasonsHeadline, trendHeadline,
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
const loaded = (rows: ApplicationRow[], reasons: ApplicantReasonCollection): LoadedApplications => ({ overall: { total: rows.length, missingDate: 0, fetchedAt: '2026-11-15T00:00:00Z', distributions: {}, byDate: byDateOf(rows) }, rows, repeats: [], reasons });
const truckReasons = collection([
  pick('t1', '給与', '2026-09-25'), pick('t2', '給与', '2026-10-20'), pick('t3', '給与', '2026-10-20'), pick('t3', '勤務地', '2026-10-20'),
  pick('t6', '給与', '2026-10-25'), pick('t4', '勤務地', '2026-10-28'), pick('t4', '会社規模', '2026-10-28', 'ouboriyuukategori_baitaikisai'), pick('t5', '給与', '2026-11-03'),
], ['t6']);
const routeReasons = collection([pick('r2', '職種興味', '2026-10-25'), pick('r3', '給与', '2026-10-25')], ['r3']);
const applicants = combineApplicants([loaded(truckRows, truckReasons), loaded(routeRows, routeReasons)], 0, 'deal');

const truck: ReportListing = {
  id: '101', title: '合成トラックドライバー', media: 'hrh', ended: false, endedOn: null, byDate: byDateOf(truckRows),
  events: { listingId: '101', bodyChangedAt: null, bodyChanges: ['2026-10-19'], imageChanges: ['2026-11-01'], startOn: '2026-08-01', statusEvents: [] },
};
const route: ReportListing = {
  id: '102', title: '合成ルート配送', media: 'hrh', ended: true, endedOn: '2026-11-05', byDate: byDateOf(routeRows),
  events: { listingId: '102', bodyChangedAt: null, bodyChanges: [], imageChanges: [], startOn: '2026-09-01', statusEvents: [{ date: '2026-11-05', kind: 'end' }] },
};
const input = (patch: Partial<ReportInput> = {}): ReportInput => ({
  scope: 'deal', id: '901', name: '合成・定期採用', company: '合成運輸', listings: [truck, route], overlap: { '2026-10-25': 1 },
  asOf: '2026-11-15', outcomes: { interviews: 4, hires: 1, readOn: '2026-11-16' }, applicants, ...patch,
});
const options = (patch: Partial<ReportOptions> = {}): ReportOptions => ({
  from: '2026-10-19', to: '2026-11-15', addressee: '合成運輸', subject: '', owner: '担当A', sender: '', createdOn: '2026-11-15', ...patch,
});
const slide = <T extends Slide['id']>(slides: Slide[], id: T) => slides.find((item): item is Extract<Slide, { id: T }> => item.id === id);

describe('見出しの1文', () => {
  it('推移は最も多い週。端数の週は見出しの対象にせず、並んだら日付を出さない', () => {
    expect(trendHeadline([{ label: '', count: 1 }, { label: '', count: 3 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('応募が最も多かったのは10月26日の週で、3件です');
    expect(trendHeadline([{ label: '', count: 5 }, { label: '', count: 3 }], 'week', ['2026-10-22', '2026-10-26'], [false, true])).toBe('応募が最も多かったのは10月26日の週で、3件です');
    expect(trendHeadline([{ label: '', count: 2 }, { label: '', count: 2 }], 'week', ['2026-10-19', '2026-10-26'])).toBe('期間中の応募は 4件で、週ごとでは最も多いときで 2件です');
    expect(trendHeadline([{ label: '', count: 0 }], 'month', ['2026-10-01'])).toBe('この期間に応募はありませんでした');
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
  it('求人をまとめ、重なる応募は1回だけ数え、前の期間と比べる（面接・採用は既定で出す。費用は出さない）', () => {
    const numbers = slide(result.slides, 'numbers');
    // 今の期間: 合成トラック 5、合成ルート配送 2。媒体の週ごとの記録が無いので、表は応募数だけ。
    expect(numbers?.hasPerf).toBe(false);
    expect(numbers?.groups).toEqual([{ name: null, period: null, rows: [
      { label: '合成トラックドライバー', impressions: null, clicks: null, ctr: null, applications: 5 },
      { label: '合成ルート配送（11/5 終了）', impressions: null, clicks: null, ctr: null, applications: 2 },
    ], total: { label: '合計', impressions: null, clicks: null, ctr: null, applications: 7 } }]);
    // 面接・採用は期間の数ではないので、期間の数字とは別の段。見出しに案件を読んだ日を書く。
    expect(numbers?.totals).toEqual({ heading: 'ご契約からの合計（11/16 時点）', kpis: [{ label: '面接', value: '4件', sub: null }, { label: '採用', value: '1名', sub: null }] });
    expect(result.excluded.some(item => item.name.includes('面接'))).toBe(false);
    // 全体は重なる応募を1回だけ数える: 今回 5 + 2 − 1 = 6、前回 1 + 1 = 2。
    expect(slide(result.slides, 'review')?.compare).toEqual([{ label: '全体', cells: ['2 → 6'] }, { label: '合成トラックドライバー', cells: ['1 → 5'] }, { label: '合成ルート配送（11/5 終了）', cells: ['1 → 2'] }]);
  });
  it('目次は入れるスライドのある章だけ（未記入の章・入れない章は出さない）', () => {
    expect(slide(result.slides, 'toc')?.items).toEqual(['掲載求人のご確認', '成果報告', 'アンケートのお願い']);
    expect(slide(buildReport(input(), options({ off: ['closing'] })).slides, 'toc')?.items).toEqual(['掲載求人のご確認', '成果報告']);
  });
  const totalsOf = (r: ReturnType<typeof buildReport>) => slide(r.slides, 'numbers')?.totals ?? null;
  const outcomeReasons = (r: ReturnType<typeof buildReport>) => r.excluded.filter(item => item.name.startsWith('数字の表（面接') || item.name.startsWith('数字の表（採用'));
  const rowsOf = (r: ReturnType<typeof buildReport>) => slide(r.slides, 'numbers')?.groups.flatMap(group => group.rows.map(row => row.applications));
  it('過去の期間で作っても、面接・採用の見出しは案件を読んだ日のまま', () => {
    const past = buildReport(input(), options({ from: '2026-09-21', to: '2026-10-18' }));
    expect(totalsOf(past)?.heading).toBe('ご契約からの合計（11/16 時点）');
  });
  it('切り替えで外すと面接・採用の段が無くなり、外した理由にも入れない（期間の数字は変わらない）', () => {
    const without = buildReport(input(), options({ include: { outcomes: false } }));
    expect(totalsOf(without)).toBeNull();
    expect(rowsOf(without)).toEqual([5, 2]);
    expect(outcomeReasons(without)).toEqual([]);
  });
  it('案件を読み込めなかったときは出さず、外した理由に「読み込めなかったため」を入れる', () => {
    const failed = buildReport(input({ outcomes: null, outcomesFailed: true }), options());
    expect(totalsOf(failed)).toBeNull();
    expect(rowsOf(failed)).toEqual([5, 2]);
    expect(outcomeReasons(failed)).toEqual([{ name: '数字の表（面接・採用）', reason: '面接・採用は読み込めなかったため外しています' }]);
  });
  it('案件に記録が無いときは出さず、外した理由に「記録が無いため」を入れる', () => {
    const empty = buildReport(input({ outcomes: { interviews: null, hires: null, readOn: '2026-11-16' } }), options());
    expect(totalsOf(empty)).toBeNull();
    expect(outcomeReasons(empty)).toEqual([{ name: '数字の表（面接・採用）', reason: '面接・採用は案件に記録が無いため外しています' }]);
  });
  it('片方だけ空なら、ある方だけ出して空の方の理由を入れる。入力された 0 はそのまま出す', () => {
    const hiresOnly = buildReport(input({ outcomes: { interviews: null, hires: 0, readOn: '2026-11-16' } }), options());
    expect(totalsOf(hiresOnly)).toEqual({ heading: 'ご契約からの合計（11/16 時点）', kpis: [{ label: '採用', value: '0名', sub: null }] });
    expect(outcomeReasons(hiresOnly)).toEqual([{ name: '数字の表（面接）', reason: '面接は案件に記録が無いため外しています' }]);
    const interviewsOnly = buildReport(input({ outcomes: { interviews: 0, hires: null, readOn: '2026-11-16' } }), options());
    expect(totalsOf(interviewsOnly)).toEqual({ heading: 'ご契約からの合計（11/16 時点）', kpis: [{ label: '面接', value: '0件', sub: null }] });
    expect(outcomeReasons(interviewsOnly)).toEqual([{ name: '数字の表（採用）', reason: '採用は案件に記録が無いため外しています' }]);
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
    expect(slide(result.slides, 'numbers')?.groups.flatMap(group => group.rows.map(row => [row.label, row.applications]))).toEqual([['合成トラックドライバー', 5], ['合成ルート配送（11/5 終了）', 2]]);
    // 掲載求人のスライドは掲載中の求人だけ
    expect(slide(result.slides, 'listings')).toMatchObject({ names: ['合成トラックドライバー'], total: 1 });
  });
  it('同じ求人名は媒体と勤務地で区別する', () => {
    const twin = buildReport(input({ listings: [{ ...truck, place: '神奈川県横浜市' }, { ...route, title: '合成トラックドライバー', ended: false, place: '東京都新宿区', media: 'airwork' }] }), options());
    expect(slide(twin.slides, 'numbers')?.groups.flatMap(group => group.rows.map(row => row.label))).toEqual(['合成トラックドライバー［HRハッカー・神奈川県横浜市］', '合成トラックドライバー［AirWork・東京都新宿区］']);
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
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'toc', 'listings', 'route', 'goals', 'numbers', 'review', 'listing:101', 'listing:102', 'trend', 'changes', 'applicants', 'reasons', 'policy', 'measure:101', 'measure:102', 'plan', 'closing']);
    // 文章が未記入のスライドは、書く場所として返すが出力しない
    expect(result.slides.filter(item => item.blank).map(item => item.id)).toEqual(['goals', 'policy', 'measure:101', 'measure:102', 'plan']);
    expect(JSON.stringify(result.slides)).not.toMatch(/未取得|HubSpot|pt|効果|合成・定期採用/);
  });
  it('表紙は宛名・題・定期MTGの日（題は既定で「運用戦略」。案件名を出さない）', () => {
    expect(slide(result.slides, 'cover')).toMatchObject({ addressee: '合成運輸 御中', title: '運用戦略', subtitle: '定期MTG 2026/11/15', footer: '担当：担当A' });
    expect(slide(buildReport(input(), options({ subject: '第2支店 運用戦略' })).slides, 'cover')?.title).toBe('第2支店 運用戦略');
  });
  it('書いた文章は行ごとの箇条書きになり、書いたスライドは出力に入る', () => {
    const full = buildReport(input(), options({ texts: { ...emptyTexts(), commonGoal: '内定 1名の獲得', planOurs: '週1回、掲載の数字を確認します\n\n必要に応じて原稿を見直します' } }));
    expect(slide(full.slides, 'plan')).toMatchObject({ blank: false, goal: '内定 1名の獲得' });
    expect(slide(full.slides, 'plan')?.columns[0]).toEqual({ title: '当社の行動計画', lines: ['週1回、掲載の数字を確認します', '必要に応じて原稿を見直します'] });
    expect(slide(full.slides, 'toc')?.items).toContain('ご提案');
  });
});

describe('外す判定', () => {
  it('一部の求人しか数えられないときは、推移と全体の比較を外す', () => {
    const airwork: ReportListing = { id: '103', title: '合成倉庫スタッフ', media: 'airwork', ended: false, endedOn: null, byDate: null, events: null };
    const result = buildReport(input({ listings: [truck, route, airwork] }), options());
    expect(result.slides.map(item => item.id)).not.toContain('trend');
    expect(slide(result.slides, 'review')?.compare.map(row => row.label)).not.toContain('全体');
    expect(result.excluded).toContainEqual({ name: '応募の推移', reason: '求人の一部しか数えられないため（応募日を読み込めなかった求人が 1件あります（合成倉庫スタッフ））' });
    const capped = buildReport(input({ partial: ['求人が多いため、先頭の50件までしか読んでいません'] }), options());
    expect(capped.slides.map(item => item.id)).not.toContain('trend');
  });
  it('応募日がどの求人も分からなければ、件数を使うスライドを外す', () => {
    const none = { ...truck, byDate: null };
    const result = buildReport(input({ listings: [none], applicants: { rows: null, reasons: null, missingReason: '一部の求人の応募者を読み込めなかったため' } }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'toc', 'listings', 'route', 'goals', 'policy', 'plan', 'closing']);
  });
  it('求人の単位では面接・採用を出さない', () => {
    const result = buildReport(input({ scope: 'listing', listings: [truck], overlap: {}, outcomes: null, applicants: combineApplicants([loaded(truckRows, truckReasons)], 0, 'listing') }), options());
    expect(result.slides.map(item => item.id)).toEqual(['cover', 'toc', 'listings', 'route', 'goals', 'numbers', 'review', 'listing:101', 'trend', 'changes', 'applicants', 'reasons', 'policy', 'measure:101', 'plan', 'closing']);
    expect(slide(result.slides, 'numbers')?.totals).toBeNull();
    // 求人の単位でも、ほかの求人に結び付いた応募の理由は外す（求人の画面と同じ）
    expect(slide(result.slides, 'reasons')?.bars).toEqual([{ label: '給与・手当', count: 3 }, { label: '勤務地・通勤', count: 2 }]);
  });
  it('最初の応募が前の期間の初めより後なら比べない', () => {
    const young = { ...truck, byDate: { '2026-10-05': 1, '2026-10-20': 2 }, events: null };
    const result = buildReport(input({ listings: [young], overlap: {} }), options());
    expect(slide(result.slides, 'review')?.compare).toEqual([{ label: '合成トラックドライバー', cells: ['— → 2'] }]);
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
