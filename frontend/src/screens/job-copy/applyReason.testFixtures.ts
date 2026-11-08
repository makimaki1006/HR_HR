import type { ApplyReasonResponse, ApplyReasonRow, OccupationStatus } from './applyReasonCrosstabModel';

/** HubSpot の選択肢 (応募動機・勤務条件の優先順位で共通、2026-11 時点の並び)。 */
export const REASON_OPTIONS = ['給与／手当', '休日／休暇', '勤務時間／シフト（夜勤なし等）', '勤務地／通勤（車通勤可等）', '仕事内容', '未経験可／研修', '雇用の安定（正社員・長期）', '職場環境／人間関係', '福利厚生（寮・食堂等）', '資格取得／キャリア', '家庭の事情', 'その他（詳細）', '回答なし'];

type Area = [string | null, string | null];
const area = ([prefecture, municipality]: Area) => ({ prefecture, municipality });
/** sel: 応募動機 (複数)、pri: 優先順位①②③ の順。 */
export function row(date: string, applicant: Area, job: Area | null, occupation: string | null, opts: { status?: OccupationStatus; sel?: string[]; pri?: string[]; texts?: string[] } = {}): ApplyReasonRow {
  return {
    date, applicant_area: area(applicant), job_area: job === null ? null : area(job), occupation,
    occupation_status: opts.status ?? (occupation === null ? 'missing' : 'grouped'),
    selections: [
      ...(opts.sel ?? []).map(label => ({ source_property: 'oubodouki_1', label })),
      ...(opts.pri ?? []).map((label, i) => ({ source_property: `kinmujoukennoyuusenjuni_${String(i + 1)}`, label })),
    ],
    texts: (opts.texts ?? []).map(text => ({ source_property: 'oubodouki', text })),
  };
}
const K: Area = ['神奈川県', '横浜市'];
const KH: Area = ['神奈川県', '平塚市'];
const T: Area = ['東京都', '世田谷区'];
const S: Area = ['埼玉県', 'さいたま市'];
const PAY = '給与／手当'; const WHERE = '勤務地／通勤（車通勤可等）'; const WORK = '仕事内容'; const OFF = '休日／休暇';

/** 今日 2026-10-21。期間の起点 1か月 09-21 / 3か月 07-21 / 6か月 04-21。最初の記録は 2026-09-18。 */
export const applyReasonRows: ApplyReasonRow[] = [
  // 1 か月以内
  row('2026-10-20', K, KH, '物流', { sel: [PAY, WHERE], pri: [PAY, OFF, WHERE], texts: ['家から近いので応募しました'] }),
  row('2026-10-15', K, K, '物流', { sel: [PAY] }),
  row('2026-10-02', T, K, '事務', { sel: [WORK], pri: [WORK] }),
  row('2026-09-25', [null, null], T, '事務', { sel: [WHERE] }),
  row('2026-09-22', K, null, '物流', { pri: [OFF, PAY] }),
  // 1 か月より前 (3・6 か月に入る)
  row('2026-09-20', S, S, '営業', { sel: [PAY, '名前が変わった選択肢'] }),
  row('2026-09-18', K, KH, null, { status: 'conflict', sel: [OFF] }),
];

export const applyReasonFixture: ApplyReasonResponse = {
  status: 'ready', built_at: '2026-10-21T04:00:00Z', refreshing: false, stale: false, refresh_failed: false,
  report: {
    today: '2026-10-21',
    options: REASON_OPTIONS,
    periods: [
      { months: 1, since: '2026-09-21', total: 2914, with_reason: 5 },
      { months: 3, since: '2026-07-21', total: 7000, with_reason: 7 },
      { months: 6, since: '2026-04-21', total: 13454, with_reason: 7 },
    ],
    occupation_groups: ['営業', 'エンジニア', '事務', '技能職', 'サービス', '介護', '医療', '販売', '物流', '製造', 'その他'],
    rows: applyReasonRows,
  },
};
