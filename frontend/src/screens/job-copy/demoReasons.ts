import type { ApplicantReason, ApplicantReasonCollection, ReasonSelection } from './applicantReasonsModel';
import { reasonSourceLabels } from './applicantReasonsModel';

/**
 * Fictional application reasons for the demo job 地域配送ドライバー (28 applications). Made-up texts
 * and choices only; no real applicant wrote them.
 */
type DemoText = [applicant: number, source: string, date: string | null, text: string];
type DemoChoice = [applicant: number, source: string, date: string | null, label: string];
const texts: DemoText[] = [
  [1, 'oubodouki', '2026-09-02', '月給が高いので'],
  [2, 'oubodouki', '2026-09-04', '家から近いため'],
  [4, 'ouboriyuu_hiaringu', '2026-09-16', '配送の仕事をしてみたい'],
  [5, 'ouboriyuu_baitaikisai', '2026-09-16', '土日休みが良い'],
  [6, 'oubodouki', '2026-09-18', '特になし'],
  [7, 'ouboriyuu_hiaringu', '2026-09-21', '時給が良いので'],
  [8, 'genshokumaeshokukaranotenshokuriyuu', '2026-09-24', '通勤に片道1時間かかるため'],
  [9, 'oubodouki', null, '大手で安定しているから'],
  [1, 'honnin_kibou_kinyuuran', '2026-09-02', '日勤のみ希望'],
  [3, 'honnin_kibou_kinyuuran', '2026-09-10', '月に2回までなら平日休みも可'],
  [8, 'kinmujoukennoyuusenjuni', '2026-09-24', '通勤時間、休日、給与の順で希望'],
  [8, 'nyuushakiboujiki', '2026-09-24', '翌月から'],
  [8, 'call_notes', '2026-09-24', '【架電】土日休みと、見学できる日程を確認したい。'],
  [1, 'shuugyoujoukyou', '2026-09-02', '在職中'],
  [1, 'oubonoheiganjoukyou', '2026-09-02', 'ほかに1件検討中'],
];
const choices: DemoChoice[] = [
  [1, 'ouboriyuukategori_hiaringu', '2026-09-02', '給与'],
  [3, 'ouboriyuukategori_baitaikisai', '2026-09-10', '勤務地'],
  [7, 'ouboriyuukategori_hiaringu', '2026-09-21', '未設定'],
  [8, 'ouboriyuukategori_hiaringu', '2026-09-24', '職種興味'],
];
const key = (applicant: number) => String(applicant).repeat(64);
const total = 28;

function build(texts: DemoText[], choices: DemoChoice[], total: number, idPrefix: string): ApplicantReasonCollection {
  const items: ApplicantReason[] = texts.map(([applicant, sourceProperty, applicationDate, text], index) => ({
    id: `${idPrefix}${'abcdef'[index % 6] ?? 'a'}${String(index)}`.padEnd(64, '0'), applicant: key(applicant), text, sourceProperty, applicationDate, collectedAt: null, versionId: null,
  }));
  const selections: ReasonSelection[] = choices.map(([applicant, sourceProperty, applicationDate, label]) => ({ applicant: key(applicant), sourceProperty, value: label, label, applicationDate }));
  const sourceCounts = Object.fromEntries(Object.keys(reasonSourceLabels).map(property => {
    const nonblank = texts.filter(row => row[1] === property).length + choices.filter(row => row[1] === property).length;
    return [property, { missing: total - nonblank, blank: 0, nonblank }];
  }));
  const missing = Object.values(sourceCounts).reduce((sum, row) => sum + row.missing, 0);
  return { available: true, basis: 'recorded_applicant_reason', fetchedAt: '2026-10-05T00:00:00Z', totalApplicants: total, totalSourceValues: total * Object.keys(reasonSourceLabels).length,
    sourceCounts, missing, blank: 0, truncated: false, items, selections, optionLabels: 'read' };
}

export function demoApplicantReasons(): ApplicantReasonCollection {
  return build(texts, choices, total, '');
}

/** Fictional reasons for the demo Airワーク job 施設清掃スタッフ (6 applications). */
const airTexts: DemoText[] = [
  [1, 'oubodouki', '2026-09-15', '自宅から近いので通いやすい'],
  [2, 'oubodouki', '2026-09-29', '時給が高いので応募しました'],
  [3, 'ouboriyuu_baitaikisai', '2026-10-02', '朝の短時間で働けるのが良い'],
  [4, 'oubodouki', null, '車で通勤できるため'],
];
const airChoices: DemoChoice[] = [
  [2, 'ouboriyuukategori_baitaikisai', '2026-09-29', '給与'],
];
export function demoAirworkReasons(): ApplicantReasonCollection {
  return build(airTexts, airChoices, 6, 'f');
}
