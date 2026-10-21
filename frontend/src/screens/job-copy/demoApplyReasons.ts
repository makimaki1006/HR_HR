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
const SELECTIONS = ['給与', '勤務地', '職種興味', '会社規模', 'その他'];

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
    const hasSel = i % 4 === 0;
    rows.push({
      date: dateBefore(TODAY, (i * 5) % 178),
      applicant_area: noArea ? { prefecture: null, municipality: null } : { prefecture: a[0], municipality: i % 13 === 2 ? null : a[1] },
      job_area: { prefecture: j[0], municipality: j[1] },
      occupation: occ, occupation_status: status,
      selections: hasSel ? [{ source_property: 'ouboriyuukategori_hiaringu', label: pick(SELECTIONS, i % SELECTIONS.length) }] : [],
      texts: hasSel && i % 8 === 0 ? [] : [{ source_property: i % 5 === 0 ? 'kinmujoukennoyuusenjuni' : 'oubodouki', text: pick(TEXTS, (i * 3) % TEXTS.length) }],
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
    periods: [
      { months: 1, since: '2026-09-21', total: 2914, with_reason: count('2026-09-21') },
      { months: 3, since: '2026-07-21', total: 7000, with_reason: count('2026-07-21') },
      { months: 6, since: '2026-04-21', total: 13454, with_reason: count('2026-04-21') },
    ],
    occupation_groups: ['営業', 'エンジニア', '事務', '技能職', 'サービス', '介護', '医療', '販売', '物流', '製造', 'その他'],
    rows,
  },
};
