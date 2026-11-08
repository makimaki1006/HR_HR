import type { ApplyReasonResponse, ApplyReasonRow, OccupationStatus } from './applyReasonCrosstabModel';

/** ?demo=1 の架空データ。地名・文はすべて架空の例。 */
const AREAS: [string, string][] = [
  ['神奈川県', '横浜市'], ['神奈川県', '川崎市'], ['神奈川県', '平塚市'],
  ['東京都', '世田谷区'], ['東京都', '足立区'], ['東京都', '八王子市'],
  ['埼玉県', 'さいたま市'], ['埼玉県', '川越市'], ['千葉県', '船橋市'], ['大阪府', '大阪市'],
];
const OCCUPATIONS = ['物流', '事務', '営業', '介護', '製造', 'サービス', '販売', '医療'];
const TEXTS = [
  '家から近いので応募しました', '時給が高いところを探していました', '未経験でも研修があると知って',
  'やってみたい仕事でした', '大手で安定した会社だと思ったから', '土日が休みなのが条件でした',
  '駅から近くて通いやすい', '給料と通勤のしやすさで選びました', '特にありません', '友人に勧められて',
];
const SELECTIONS = ['給与／手当', '休日／休暇', '勤務時間／シフト（夜勤なし等）', '勤務地／通勤（車通勤可等）', '仕事内容', '未経験可／研修', '雇用の安定（正社員・長期）', '職場環境／人間関係', '福利厚生（寮・食堂等）', '資格取得／キャリア', '家庭の事情', 'その他（詳細）', '回答なし'];

const pick = <T,>(xs: readonly T[], i: number): T => xs[i % xs.length] as T;

function dateBefore(today: string, days: number): string {
  const d = new Date(`${today}T00:00:00Z`); d.setUTCDate(d.getUTCDate() - days); return d.toISOString().slice(0, 10);
}
const TODAY = '2026-10-21';

function build(): ApplyReasonRow[] {
  const rows: ApplyReasonRow[] = [];
  for (let i = 0; i < 72; i++) {
    const a = pick(AREAS, (i * 7 + (i % 3)) % AREAS.length);
    const j = pick(AREAS, (i * 3 + 1) % AREAS.length);
    const noArea = i % 17 === 5;
    const occ = i % 11 === 4 ? null : pick(OCCUPATIONS, (i * 5) % OCCUPATIONS.length);
    const status: OccupationStatus = occ !== null ? 'grouped' : (['missing', 'unmatched', 'conflict'] as const)[i % 3] as OccupationStatus;
    const motive = [pick(SELECTIONS, (i * 3) % 6), ...(i % 3 === 0 ? [pick(SELECTIONS, (i + 4) % 10)] : [])];
    const priority = i % 2 === 0 ? [pick(SELECTIONS, i % 7), pick(SELECTIONS, (i + 2) % 9), pick(SELECTIONS, (i + 5) % 11)] : [];
    rows.push({
      date: dateBefore(TODAY, (i * 5) % 50),
      applicant_area: noArea ? { prefecture: null, municipality: null } : { prefecture: a[0], municipality: i % 13 === 2 ? null : a[1] },
      job_area: { prefecture: j[0], municipality: j[1] },
      occupation: occ, occupation_status: status,
      selections: [
        ...[...new Set(motive)].map(label => ({ source_property: 'oubodouki_1', label })),
        ...priority.map((label, n) => ({ source_property: `kinmujoukennoyuusenjuni_${String(n + 1)}`, label })),
      ],
      texts: i % 4 === 0 ? [{ source_property: 'oubodouki', text: pick(TEXTS, (i * 3) % TEXTS.length) }] : [],
    });
  }
  return rows;
}
const rows = build();
const count = (since: string) => rows.filter(r => r.date >= since).length;

export const demoApplyReasons: ApplyReasonResponse = {
  status: 'ready', built_at: '2026-10-21T04:00:00Z', refreshing: false, stale: false, refresh_failed: false,
  report: {
    today: TODAY,
    options: SELECTIONS,
    periods: [
      { months: 1, since: '2026-09-21', total: 2914, with_reason: count('2026-09-21') },
      { months: 3, since: '2026-07-21', total: 7000, with_reason: count('2026-07-21') },
      { months: 6, since: '2026-04-21', total: 13454, with_reason: count('2026-04-21') },
    ],
    occupation_groups: ['営業', 'エンジニア', '事務', '技能職', 'サービス', '介護', '医療', '販売', '物流', '製造', 'その他'],
    rows,
  },
};
