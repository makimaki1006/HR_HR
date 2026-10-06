// テスト用の架空データ (実在の求人・企業・人名は含めない)。型は ts-rs の生成物に合わせる。
import type { CompetitorReport } from '../../generated/CompetitorReport';
import type { CompetitorReportResponse } from '../../generated/CompetitorReportResponse';

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
        rows: [
          { word: '未経験歓迎', head_count: 30, head_share_pct: 66.666, all_count: 400, all_share_pct: 32.4 },
          { word: '賞与あり', head_count: 9, head_share_pct: 20, all_count: null, all_share_pct: null },
        ],
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
            monthly_12m: [
              { month: '2025-10', search_volume: 1800 },
              { month: '2025-11', search_volume: 0 },
              { month: '2025-12', search_volume: null },
            ],
          },
          { keyword: '別の語', avg_monthly: null, competition: 'LOW', monthly_12m: [] },
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
      title: 'テスト職',
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
