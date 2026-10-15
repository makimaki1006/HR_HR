import type { HomeCard, HomeResponse, HomeRowData } from './homeModel';

// ホームの表示確認用。すべて架空の求人・顧客。
const row = (id: string, title: string, prev: number, recent: number, cost: number): HomeRowData => ({ id, title, company: '合成事業所', media: 'hrh', prev_apps: prev, recent_apps: recent, recent_cost: cost, latest_week_end: '2026-10-11' });
const zero = row('demo-zero', '合成求人・配送', 0, 0, 24000);
const stopped = row('demo-stopped', '合成求人・倉庫', 1, 0, 12000);
const less = row('demo-less', '合成求人・事務', 5, 4, 8000);
const more = row('demo-more', '合成求人・販売', 0, 1, 4000);
const flat = { ...row('demo-flat', '合成求人・受付', 10, 9, 6000), latest_week_end: '2026-09-27' };
const card = (key: HomeCard['key'], title: string, rule: string, listings: HomeRowData[]): HomeCard => ({ key, title, rule, count: listings.length, listings });
export const demoHome: HomeResponse & { status: 'ready' } = {
  status: 'ready', index_built_at: '2026-10-15T01:00:00Z', today: '2026-10-15', companies_ok: true,
  rules: { trend_min_diff: 2, trend_min_percent: 20, cost_floor_yen: 10000, cost_per_app_times: 2, stale_days: 14, list_cap: 100 },
  home: {
    cards: [
      card('zero_stay', '応募がゼロのままの求人', '掲載中で、前の4週も直近4週も応募が0件の求人。直近4週の課金が多い順に表示します。', [zero, row('demo-zero-free', '合成求人・清掃', 0, 0, 0)]),
      card('to_zero', '応募がゼロになった求人', '掲載中で、前の4週に応募が1件以上あり、直近4週は0件になった求人。', [stopped]),
      card('less', '直近4週の応募が、前の4週より少ない求人', '掲載中で、直近4週の応募が1件以上あり、前の4週より2件以上、または20%以上少ない求人。', [less]),
      card('more', '直近4週の応募が、前の4週より多い求人', '掲載中で、直近4週の応募が前の4週より2件以上、または20%以上多い求人。前の4週が0件で応募があった求人も含みます。', [more]),
      card('flat', '応募はあるが、前の4週とほぼ同じ求人', '掲載中で、直近4週に応募があり、前の4週と同じ件数、または差が2件未満かつ20%未満の求人。', [flat]),
      card('costly', '課金が高いまま応募が少ない求人', '掲載中で、直近4週の課金が1万円以上あり、応募が0件、または応募1件あたりの課金が応募のある求人の中央値の2倍以上の求人。', [zero, stopped]),
      card('stale', 'データが止まっている求人', '掲載中で、週ごとの実績の最新の週が14日より前に終わっている求人。この間の応募・課金は少なく見えます。', [flat]),
      card('occupation', '市場の職種が未指定の求人', '掲載中で、市場と比べる職種をまだ指定していない求人。', [less]),
    ],
    customers: [{ company_id: 'demo-company', name: '合成事業所', deals: 1, active_deals: 1, latest_deal: { name: '合成案件', stage_label: '契約中', active: true, created_at: '2026-10-01' }, live_listings: 6, total_listings: 6, prev_apps: 16, recent_apps: 14, recent_cost: 54000, cost_per_app: 3857 }],
    live_without_company: 0, median_cost_per_app: 2000,
  },
};
