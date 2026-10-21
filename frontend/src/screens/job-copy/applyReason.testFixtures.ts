import type { ApplyReasonResponse, ApplyReasonRow, OccupationStatus } from './applyReasonCrosstabModel';

type Area = [string | null, string | null];
const area = ([prefecture, municipality]: Area) => ({ prefecture, municipality });
export function row(date: string, applicant: Area, job: Area, occupation: string | null, opts: { status?: OccupationStatus; sel?: string[]; texts?: string[] } = {}): ApplyReasonRow {
  return {
    date, applicant_area: area(applicant), job_area: area(job), occupation,
    occupation_status: opts.status ?? (occupation === null ? 'missing' : 'grouped'),
    selections: (opts.sel ?? []).map(label => ({ source_property: 'ouboriyuukategori_hiaringu', label })),
    texts: (opts.texts ?? []).map(text => ({ source_property: 'oubodouki', text })),
  };
}
const K: Area = ['神奈川県', '横浜市'];
const KH: Area = ['神奈川県', '平塚市'];
const T: Area = ['東京都', '世田谷区'];
const S: Area = ['埼玉県', 'さいたま市'];

/** 今日 2026-10-21。期間の起点 1か月 09-21 / 3か月 07-21 / 6か月 04-21。 */
export const applyReasonRows: ApplyReasonRow[] = [
  // 1 か月以内 (4 件)
  row('2026-10-20', K, KH, '物流', { sel: ['給与'], texts: ['家から近いので応募しました'] }),
  row('2026-10-15', K, K, '物流', { texts: ['時給が高いと思いました'] }),
  row('2026-10-02', T, K, '事務', { texts: ['特にありません'] }),
  row('2026-09-25', [null, null], T, '事務', { sel: ['職種興味'] }),
  // 3 か月以内 (+3)
  row('2026-08-10', K, K, '物流', { texts: ['通勤しやすい場所だった'] }),
  row('2026-08-01', K, KH, '物流', { texts: ['給料と家から近いのが決め手'] }),
  row('2026-07-30', T, T, null, { status: 'unmatched', texts: ['大手で安定した会社だから'] }),
  // 6 か月以内 (+4)
  row('2026-06-15', S, S, '営業', { sel: ['会社規模', '給与'] }),
  row('2026-05-20', K, K, '物流', { texts: ['未経験でも研修があるから'] }),
  row('2026-05-02', S, [null, null], null, { status: 'conflict', texts: ['やってみたい仕事でした'] }),
  row('2026-04-22', K, KH, '営業', { sel: ['未設定'], texts: ['駅から近い'] }),
  // 6 か月より前 (期間外)
  row('2026-03-30', K, K, '物流', { sel: ['給与'] }),
];

export const applyReasonFixture: ApplyReasonResponse = {
  status: 'ready', built_at: '2026-10-21T04:00:00Z', refreshing: false, stale: false, refresh_failed: false,
  report: {
    today: '2026-10-21',
    periods: [
      { months: 1, since: '2026-09-21', total: 2914, with_reason: 4 },
      { months: 3, since: '2026-07-21', total: 7000, with_reason: 7 },
      { months: 6, since: '2026-04-21', total: 13454, with_reason: 11 },
    ],
    occupation_groups: ['営業', 'エンジニア', '事務', '技能職', 'サービス', '介護', '医療', '販売', '物流', '製造', 'その他'],
    rows: applyReasonRows,
  },
};
