import type { CompetitorReport } from '../../generated/CompetitorReport';
// テスト用の架空データ (実在の求人・企業・人名は含めない)。型は ts-rs の生成物に合わせる。
import type { CompetitorReportResponse } from '../../generated/CompetitorReportResponse';
import type { GoogleMonth } from '../../generated/GoogleMonth';
import type { PopularitySplit } from '../../generated/PopularitySplit';

/** 開始 (年, 月) から n か月分の月次検索数。値は f(i)。 */
export function makeMonthly(start: [number, number], n: number, f: (i: number) => number | null): GoogleMonth[] {
  return Array.from({ length: n }, (_, i) => {
    const t = start[0] * 12 + (start[1] - 1) + i;
    return {
      month: `${String(Math.floor(t / 12))}-${String((t % 12) + 1).padStart(2, '0')}`,
      search_volume: f(i),
    };
  });
}

export function makeReport(): CompetitorReport {
  return {
    meta: {
      title: 'テスト調査',
      employment_type: '正社員',
      prefecture: '大阪府',
      municipality: '大阪市',
      unit: '万円',
      is_hourly: false,
      total_count: 1234,
      top_n_effective: 45,
      top_n_requested: '45',
      salary_parsed_count: 1200,
      salary_missing_count: 34,
      warnings: [],
    },
    excel: {
      decimals: 2,
      salary_table: [
        { label: '平均値', values: [25.5, 32.125, 27, null] },
        { label: '中央値', values: [24, 30, 0, 31] },
        { label: '最頻値', values: [22, 28, 26, 33] },
      ],
      salary_counts: [1200, 1190, 20, 18],
      salary_diff: [
        { label: '平均値', values: [-1.5, null] },
        { label: '中央値', values: [0, -1] },
      ],
      keyword_all: Array.from({ length: 12 }, (_, i) => ({
        word: `ワード${String(i + 1)}`,
        count: 100 - i * 7,
        jobs: 1234,
        share_pct: (100 - i * 7) / 12.34,
      })),
      keyword_head: [{ word: '未経験歓迎', count: 30, jobs: 45, share_pct: 66.666 }],
      keyword_comparison: {
        head_n: 45,
        all_n: 1234,
        popular_any_n: 0,
        rows: [
          { word: '未経験歓迎', head_count: 30, head_share_pct: 66.666, all_count: 400, all_share_pct: 32.4, popular_any_count: null, popular_any_share_pct: null },
          { word: '賞与あり', head_count: 9, head_share_pct: 20, all_count: null, all_share_pct: null, popular_any_count: null, popular_any_share_pct: null },
        ],
      },
      tag_usage: {
        n: 1234,
        popular_n: 40,
        bands: [
          {
            key: 'most',
            label: 'ほとんどの求人が付けている',
            range: '全体の50%以上',
            rows: [{ word: '交通費支給', n_tag: 700, share_pct: 56.7, popular_tag_n: 30, popular_share_pct: 75, gap_pt: 18.3 }],
          },
          {
            key: 'half',
            label: '半数前後',
            range: '全体の20%以上50%未満',
            rows: [{ word: '昇給あり', n_tag: 400, share_pct: 32.4, popular_tag_n: 8, popular_share_pct: 20, gap_pt: -12.4 }],
          },
          {
            key: 'some',
            label: '一部の求人だけ',
            range: '全体の20%未満（3件以上）',
            rows: [{ word: '車通勤OK', n_tag: 50, share_pct: 4.05, popular_tag_n: 6, popular_share_pct: 15, gap_pt: 10.95 }],
          },
        ],
        minor_word_count: 7,
        popular_up: [
          { word: '交通費支給', n_tag: 700, share_pct: 56.7, popular_tag_n: 30, popular_share_pct: 75, gap_pt: 18.3 },
          { word: '車通勤OK', n_tag: 50, share_pct: 4.05, popular_tag_n: 6, popular_share_pct: 15, gap_pt: 10.95 },
        ],
        popular_down: [{ word: '昇給あり', n_tag: 400, share_pct: 32.4, popular_tag_n: 8, popular_share_pct: 20, gap_pt: -12.4 }],
      },
      histograms: {
        upper: {
          bins: [
            { label: '20', count: 3 },
            { label: '21', count: 10 },
            { label: '22', count: 5 },
            { label: '23', count: 0 },
          ],
          n: 18,
          step: 1,
          summary: '最多の給与帯：21〜22 / 10件・55.6%',
        },
        lower: {
          bins: [
            { label: '18', count: 2 },
            { label: '19', count: 4 },
          ],
          n: 6,
          step: 1,
          summary: '最多の給与帯：19〜20 / 4件・66.7%',
        },
      },
    },
    google: {
      status: 'ok',
      keyword: 'テスト職 求人',
      region: '大阪府',
      demand: {
        status: 'ok',
        region_name: 'Osaka',
        keywords: [
          {
            keyword: 'テスト職 求人',
            avg_monthly: 1900,
            competition: 'HIGH',
            monthly: makeMonthly([2022, 10], 48, (i) => 1500 + (i % 12) * 50),
          },
          {
            keyword: 'テスト職 募集',
            avg_monthly: 800,
            competition: 'MEDIUM',
            monthly: makeMonthly([2022, 10], 48, (i) => 700 + (i % 12) * 20),
          },
          { keyword: '別の語', avg_monthly: null, competition: 'LOW', monthly: [] },
        ],
      },
      suggestions: {
        status: 'ok',
        region_name: null,
        suggestions: [
          { keyword: '関連語A', avg_monthly: 500 },
          { keyword: '関連語B', avg_monthly: 0 },
          { keyword: '関連語C', avg_monthly: null },
        ],
      },
    },
    indeed: {
      status: 'ok',
      region: '大阪府',
      source: 'テスト出典',
      caveat: 'テスト注意書き',
      built_at: '2026-09-30',
      rows: [
        { month: '2026-07', job: 12000, ctk: 345.5, emp: 0, spp: 1.25 },
        { month: '2026-08', job: null, ctk: null, emp: null, spp: null },
      ],
    },
    population: {
      status: 'ok',
      region: '大阪府',
      is_national: false,
      reference_date: '2020-10-01',
      shares: null,
      bands: [
        { age_group: '0～4歳', male: 100000, female: 95000 },
        { age_group: '5～9歳', male: 110000, female: 104000 },
        { age_group: '10～14歳', male: 120000, female: 0 },
      ],
      minimum_wage: 1064,
      minimum_wage_fiscal_year: 2025,
      minimum_wage_effective_date: '2025-10-16',
      minimum_wage_as_of: '2026-10-05',
      minimum_wage_source: 'official_csv',
      minimum_wage_source_url: null,
      labor: { fiscal_year: 2024, unemployment_rate: 2.5, separation_rate: null },
    },
    popularity_split: null,
    consultation: {
      cohort: '総合：Indeed SPの月給求人の実額（月給換算は含みません）',
      salary: [
        { label: '下限', all_median: 24, all_n: 1200, popular_median: 0, popular_n: 20, delta: 24 },
        { label: '上限', all_median: 30, all_n: 1190, popular_median: null, popular_n: 18, delta: null },
      ],
      distribution_min_n: 1200,
      distribution_max_n: 1190,
      small_sample: false,
      gaps: {
        status: 'rows',
        head_n: 45,
        all_n: 1234,
        rows: [
          { word: '賞与あり', head: 9, all: 700, head_share_pct: 20, all_share_pct: 56.7, points: -36.7 },
        ],
      },
      external: [
        { label: 'Google検索需要', fetched: true },
        { label: 'Google関連語', fetched: true },
        { label: 'Indeed採用市場', fetched: true },
        { label: '人口・地域', fetched: true },
      ],
    },
  };
}

export function makeResponse(report: CompetitorReport = makeReport()): CompetitorReportResponse {
  return { report_id: 'a'.repeat(64), expires_in_secs: 1800, report };
}

// 人気の分かれ方 (契約 report.popularity_split)。架空の値。
export function makePopularitySplit(): PopularitySplit {
  return {
    total: 325,
    popular: 106,
    super_popular: 20,
    by_age: [
      { label: '0〜14日', n: 82, popular: 19, super_popular: 5 },
      { label: '15〜29日', n: 61, popular: 30, super_popular: 4 },
      { label: '30日以上', n: 182, popular: 57, super_popular: 11 },
      { label: '不明', n: 0, popular: 0, super_popular: 0 },
    ],
    tags_by_age: [
      { label: '0〜14日', tags: [{ tag: 'ブランクOK', n: 12, popular: 7 }] },
      { label: '15〜29日', tags: [{ tag: '服装自由', n: 20, popular: 9 }] },
      { label: '30日以上', tags: [{ tag: '転勤なし', n: 30, popular: 14 }] },
    ],
    super_recent: {
      n_super: 5,
      n_base: 77,
      tags: [{ tag: '研修あり', super_pct: 60, base_pct: 22.5 }],
      credential_share: { super_pct: 40, base_pct: 10.25 },
    },
    companies: {
      rows: [
        { name: 'テスト工業', n: 6, popular: 3, super_popular: 2, longest_days: null, longest_is_30plus: true },
        { name: 'サンプル商事', n: 4, popular: 0, super_popular: 0, longest_days: 25, longest_is_30plus: false },
      ],
      multi: { companies: 48, n: 137, popular: 36 },
      single: { companies: 0, n: 0, popular: 0 },
    },
    company_bias: {
      total: 325,
      popular: 106,
      judged_companies: 19,
      biased_companies: 2,
      biased_popular: 9,
      biased_popular_pct: 8.49,
      biased_n: 11,
      biased_n_pct: 3.38,
      fewer_companies: 1,
      fallback_no_age: 83,
      fallback_salary_only: 58,
      fallback_overall: 2,
      rows: [
        { name: 'テスト工業', n: 6, popular: 5, expected: 1.62, diff: 3.38, verdict: '会社に偏りがある' },
        { name: 'サンプル商事', n: 4, popular: 1, expected: 1.04, diff: -0.04, verdict: '給与・条件の範囲' },
      ],
      after_total: 314,
      after_popular: 97,
      tags_more: [{ tag: '学歴不問', n: 67, before: 16.3, after: 15.1 }],
      tags_less: [{ tag: '寮完備', n: 8, before: -4.2, after: null }],
    },
    reach: {
      open_words: ['学歴不問', '未経験者歓迎', 'ブランクOK', 'フリーター歓迎', '第二新卒歓迎'],
      min_band_n: 30,
      bands: [
        { label: 'なし', n: 200, popular: 50, super_popular: 8, n_share_pct: 61.5, popular_share_pct: 47.2, rate_pct: 25 },
        { label: '1つ', n: 45, popular: 14, super_popular: 3, n_share_pct: 13.8, popular_share_pct: 13.2, rate_pct: 31.1 },
        { label: '2つ以上', n: 80, popular: 42, super_popular: 9, n_share_pct: 24.6, popular_share_pct: 39.6, rate_pct: 52.5 },
      ],
      strata: [
        { label: '0〜3個', without_open: { n: 120, popular: 20, rate_pct: 16.7 }, with_open: { n: 0, popular: 0, rate_pct: null } },
        { label: '4〜6個', without_open: { n: 60, popular: 16, rate_pct: 26.7 }, with_open: { n: 50, popular: 29, rate_pct: 58 } },
        { label: '7個以上', without_open: { n: 20, popular: 5, rate_pct: 25 }, with_open: { n: 75, popular: 27, rate_pct: 36 } },
      ],
      band_tags: [
        {
          label: 'なし',
          n: 200,
          popular: 50,
          shown: true,
          more: [{ tag: '長期休暇あり', n: 40, diff: 12.5 }],
          less: [{ tag: '昇給あり', n: 30, diff: -8.3 }],
        },
        { label: '1つ', n: 45, popular: 14, shown: true, more: [], less: [] },
        { label: '2つ以上', n: 25, popular: 10, shown: false, more: [], less: [] },
      ],
    },
    salary: {
      popular_median: 220000,
      popular_n: 106,
      other_median: null,
      other_n: 219,
      bands: [{ label: '〜20.5万', n: 67, popular: 21, super_popular: 3 }],
    },
    tags: {
      welfare: [{ tag: '食事補助あり', n: 13, popular: 8 }],
      easy_apply: [{ tag: '主婦・主夫歓迎', n: 10, popular: 8 }],
      count_buckets: [{ label: '0個', n: 23, popular: 1, super_popular: 0 }],
    },
    writing: [{ title: '見出しの長さ別', rows: [{ label: '26〜40字', n: 105, popular: 52, super_popular: 9 }] }],
    clarity: [{ label: '4点', n: 72, popular: 35, super_popular: 6 }],
    work_style: [{ label: '夜勤', n: 17, popular: 12, super_popular: 3 }],
  };
}
