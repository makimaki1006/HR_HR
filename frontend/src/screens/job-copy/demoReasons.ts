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
];
const choices: DemoChoice[] = [
  [1, 'ouboriyuukategori_hiaringu', '2026-09-02', '給与'],
  [3, 'ouboriyuukategori_baitaikisai', '2026-09-10', '勤務地'],
  [7, 'ouboriyuukategori_hiaringu', '2026-09-21', '未設定'],
  [8, 'ouboriyuukategori_hiaringu', '2026-09-24', '職種興味'],
];
const key = (applicant: number) => String(applicant).repeat(64);
const total = 28;

export function demoApplicantReasons(): ApplicantReasonCollection {
  const items: ApplicantReason[] = texts.map(([applicant, sourceProperty, applicationDate, text], index) => ({
    id: `${'abcdef'[index % 6] ?? 'a'}${String(index)}`.padEnd(64, '0'), applicant: key(applicant), text, sourceProperty, applicationDate, collectedAt: null, versionId: null,
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
