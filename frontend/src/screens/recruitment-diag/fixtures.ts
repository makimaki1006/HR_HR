// Mock API bodies for the tests. Each one is checked against the type generated from the Rust
// struct (satisfies), so a field renamed or removed on the Rust side fails `npm run typecheck`.
import type { RdCompetitorsResponse } from '../../generated/RdCompetitorsResponse';
import type { RdConditionGapResponse } from '../../generated/RdConditionGapResponse';
import type { RdDifficultyResponse } from '../../generated/RdDifficultyResponse';
import type { RdInsightsResponse } from '../../generated/RdInsightsResponse';
import type { RdMarketTrendResponse } from '../../generated/RdMarketTrendResponse';
import type { RdOpportunityMapResponse } from '../../generated/RdOpportunityMapResponse';
import type { RdTalentPoolExpansionResponse } from '../../generated/RdTalentPoolExpansionResponse';
import type { RdTalentPoolResponse } from '../../generated/RdTalentPoolResponse';

export const difficulty = {
  panel: 'difficulty_score',
  inputs: { job_type: '老人福祉・介護', emp_type: '正社員', prefecture: '東京都', municipality: '', citycode: null },
  metrics: {
    hw_count: 12345,
    population: 987654,
    day_population: 1000000,
    night_population: 900000,
    day_night_ratio: 1.11,
    is_tourist_area: false,
    score_per_10k: 1.6666,
    national_hw_count: 1004000,
    area_share_of_national: 0.0123,
  },
  rank: 4,
  rank_label: '激戦',
  so_what: '採用競合が多い傾向があります。',
  tourist_correction_note: null,
  notes: {
    hw_scope: 'HW のみ',
    causation: '相関であり因果ではない',
    calculation: 'HW 求人数 ÷ 昼間人口 × 10000',
    population_year: 2021,
    tourist_threshold: 1.5,
  },
} satisfies RdDifficultyResponse;

export const talentPool = {
  panel: 'talent_pool',
  inputs: { prefecture: '東京都', municipality: '新宿区', citycode: 13104, year: 2021 },
  metrics: {
    day_population: 1234567,
    night_population: 1300000,
    commuter_inflow: -65433,
    day_night_ratio: 0.9497,
  },
  so_what: 'ベッドタウン型',
  notes: { hw_scope: 'HW のみ', causation: '相関であり因果ではない', data_source: 'Agoop', method: '昼 - 夜' },
} satisfies RdTalentPoolResponse;

export const competitors = {
  prefecture: '東京都',
  municipality: '',
  job_type: '老人福祉・介護',
  companies: [
    {
      corporate_number: '1234567890123',
      name: 'テスト株式会社',
      prefecture: '東京都',
      sn_industry: '介護',
      employees: 12345,
      sales_amount: 987654,
      sales_range: '10億〜50億',
      credit_score: 72.456,
      hw_postings_count: 1234,
    },
    {
      corporate_number: '',
      name: '番号なし法人',
      prefecture: '東京都',
      sn_industry: '',
      employees: 50,
      sales_amount: 0,
      sales_range: '',
      credit_score: 0,
      hw_postings_count: 3,
    },
  ],
  top20_insight: '',
  warning: 'HW のみ',
  mapping_confidence: 0.9,
  mapping_warning: null,
} satisfies RdCompetitorsResponse;

export const conditionGap = {
  prefecture: '東京都',
  municipality: '',
  job_type: '老人福祉・介護',
  emp_type: '正社員',
  industry_median: { annual_income: 4500000, annual_holidays: 115, bonus_months: 2.4, sample_size: 12345 },
  all_industry_median: { annual_income: 4000000, annual_holidays: 120, bonus_months: 0, sample_size: 456789 },
  // monthly 25万円 x (12 + 2.0) = 350万円, 125 days, bonus 2.0
  company: { annual_income_estimated: 3500000, annual_holidays: 125, bonus_months: 2, salary_min: 250000 },
  gap_industry: { annual_income_diff: -1000000, annual_income_pct: -22.2, annual_holidays_diff: 10, bonus_months_diff: -0.4 },
  gap_all: { annual_income_diff: -500000, annual_income_pct: -12.5, annual_holidays_diff: 5, bonus_months_diff: 2 },
  interpretation: '年収は業界中央値を下回る傾向があります。',
  warning: 'HW のみ',
} satisfies RdConditionGapResponse;

export const marketTrend = {
  prefecture: '東京都',
  job_type: '老人福祉・介護',
  emp_type: '正社員',
  months_requested: 6,
  months: ['2026-04', '2026-05', '2026-06'],
  counts: [100, 110, 125],
  growth_rate_pct: 25,
  metric_label: '業界サンプル件数',
  is_sample: true,
  data_source: 'ts_turso_salary',
  interpretation: '増加傾向があります。',
  warning: 'HW のみ',
} satisfies RdMarketTrendResponse;

export const opportunity = {
  prefcode: 13,
  filters: { job_type: '老人福祉・介護', emp_type: '正社員' },
  municipalities: [
    { name: '激戦区', citycode: 13101, hw_count: 900, population: 100000, score: 90, category: '激戦' },
    { name: '穴場区', citycode: 13102, hw_count: 10, population: 200000, score: 0.5, category: '穴場' },
    { name: '標準区', citycode: 13103, hw_count: 100, population: 100000, score: 10, category: '標準' },
  ],
  legend: {
    opportunity: { label: '穴場', max: 5, color: '#22c55e' },
    standard: { label: '標準', min: 5, max: 20, color: '#64748b' },
    competitive: { label: '激戦', min: 20, color: '#ef4444' },
    unit: '人口1万人あたり',
  },
  note: 'HW のみ',
} satisfies RdOpportunityMapResponse;

const insight = (pattern_id: string, severity: string, severity_rank: number) => ({
  pattern_id,
  category: 'HS',
  severity,
  severity_rank,
  title: `title-${pattern_id}`,
  message: `message-${pattern_id}`,
  evidence: [{ metric: 'm', value: 1, unit: '%', context: 'c' }],
  related_tabs: ['market'],
  hr_action: severity_rank === 3 ? '' : `action-${pattern_id}`,
});

export const insights = {
  prefcode: 13,
  citycode: null,
  pref: '東京都',
  municipality: '',
  filters: { job_type: null, emp_type: null },
  insights: [insight('P0', '重大', 0), insight('P1', '注意', 1), insight('P2', '情報', 2), insight('P3', '良好', 3)],
  summary: '4 件',
  note: 'HW のみ',
} satisfies RdInsightsResponse;

export const expansion = {
  panel: 'talent_pool_expansion',
  current: { prefecture: '東京都', municipality: '新宿区' },
  tier_30min: {
    municipality_count: 2,
    unemployment_pool: 12345,
    hw_postings: 678,
    breakdown: [
      { prefecture: '東京都', municipality: '渋谷区', commuters: 5000, unemployment: 111, hw_postings: 22 },
      { prefecture: '東京都', municipality: '中野区', commuters: 4000, unemployment: 222, hw_postings: 33 },
    ],
  },
  tier_60min: { municipality_count: 0, unemployment_pool: 0, hw_postings: 0, breakdown: [] },
  is_data_available: true,
  notes: {
    hw_scope: 'HW のみ(API)',
    causation: '相関であり因果ではない',
    data_source_od: 'OD(API)',
    data_source_unemployment: '失業(API)',
    data_source_hw: 'HW(API)',
    tier_definition: '分圏(API)',
    caveat_pool: '応募意向ではない(API)',
    caveat_distance: '距離ではない(API)',
  },
} satisfies RdTalentPoolExpansionResponse;
