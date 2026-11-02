// 画面のテスト・スクリーンショット用の架空データ。実在の会社・人・取引 ID は入れない。
// 形は /api/consulting/* の応答 (src/handlers/cs_dashboard/routes.rs) に合わせてある。
import type { BoardResponse, CustomerResponse, DealRow, DetailResponse, GoalsResponse, MtgQualityResponse, PhoneResponse, RenewalPipeResponse, ResultsResponse, TeamResponse, TodayResponse } from '../types';

const base = {
  retired: false,
  focus: false,
  past_expiry: false,
  not_started: false,
  overdue_active: false,
  period: 6,
  band: '半ば',
  phase: 3,
  phase_key: '3',
  phase_label: '3 更新の判断',
  mtg_source: 'recording',
  mtg_source_label: 'Zoom 録画（事実）',
};

export const DEAL_A: DealRow = {
  ...base,
  deal_id: '70000000001',
  name: '架空株式会社A 本社',
  stage: '稼働中',
  consultant: '架空担当あお',
  amount: 1_200_000,
  focus: true,
  flags: ['満足度が4以下', 'MTGが60日以上空いている', '満了まで60日以内'],
  n_flags: 3,
  months: 5,
  days_left: 28,
  nps: 3,
  nps_month: '2026-09',
  n_contact: 2,
  days_since_contact: 41,
  mtg_band: 'critical',
  mtg_days: 72,
  expiry: '2026-11-30',
  start: '2026-06-01',
  oubo: 12,
  oubo_carry: false,
  mensetu: 3,
  mensetu_carry: true,
  syoudaku: 1,
  syoudaku_carry: false,
  saiyomokuhyou: 4,
  rate_tassei: 0.25,
  contact_touched: 2,
  contact_months: 5,
  contact_rate: 40,
  cpa: 1_200_000,
  cpa_band: '前半',
  cpa_band_median: 800_000,
  cpa_vs_band: 1.5,
};
export const DEAL_B: DealRow = {
  ...base,
  deal_id: '70000000002',
  name: '架空株式会社B 第二拠点',
  stage: '稼働中',
  consultant: '架空担当みどり',
  amount: 800_000,
  flags: ['接触の記録が無い'],
  n_flags: 1,
  months: 2,
  days_left: 120,
  nps: null,
  n_contact: 0,
  days_since_contact: null,
  mtg_band: 'no_record',
  mtg_days: null,
  expiry: '2027-03-01',
  start: '2026-09-01',
};
export const DEAL_C: DealRow = {
  ...base,
  deal_id: '70000000003',
  name: '架空株式会社C 支店',
  stage: '稼働中',
  consultant: '架空担当みどり',
  amount: null,
  flags: [],
  n_flags: 0,
  phase: null,
  phase_key: 'overdue',
  past_expiry: true,
  overdue_active: true,
  months: null,
  days_left: -12,
  nps: 9,
  nps_month: '2026-10',
  n_contact: 5,
  days_since_contact: 5,
  mtg_band: 'recent',
  mtg_days: 9,
  expiry: '2026-10-21',
  start: '2026-04-21',
};

const ROWS = [DEAL_A, DEAL_B, DEAL_C];

export const BOARD: BoardResponse = {
  meta: {
    today: '2026-11-02',
    hubspot_portal_id: '12345',
    n_active: 3,
    flag_counts: [
      { label: '接触の記録が無い', n: 1 },
      { label: '満足度が4以下', n: 1 },
      { label: 'MTGが60日以上空いている', n: 1 },
      { label: '満了まで60日以内', n: 1 },
    ],
    mtg_gap: { bands: [{ band: 'critical', label: '重大（60日以上）', n: 1, alert: true }] },
  },
  rows: ROWS,
};

export const TODAY: TodayResponse = {
  meta: { today: '2026-11-02', hubspot_portal_id: '12345', n_hit: 2, n_shown: 2, n_flags2: 1, filter_rule: '候補は 2 件です（架空の説明文）。' },
  rows: [DEAL_A, DEAL_B],
  candidates: [DEAL_A, DEAL_B],
  expiring_this_week: [],
  started_this_week: [DEAL_B],
  not_started: [],
  consultants: ['架空担当あお', '架空担当みどり'],
};

export const TEAM: TeamResponse = {
  meta: { source_as_of: '2026-10-26 14:01:00', source_age_days: 7, generated_at: '2026-10-26 14:05:00', update_schedule: '毎日 21:30', n_consultant: 2, n_active: 3, retired_deals: 0, retired_people: 0, unknown_owner: 0 },
  status: {
    rows: [
      { consultant: '架空担当あお', n_active: 1, unattended: 1, n_flags2: 1, mtg_critical: 1, no_contact: 0, expiring60: 1, nps_low: 1, expiring_week: 0, delta: { n_flags2: 1 } },
      { consultant: '架空担当みどり', n_active: 2, unattended: 2, n_flags2: 0, mtg_critical: 0, no_contact: 1, expiring60: 0, nps_low: 0, expiring_week: 0 },
    ],
    meta: {
      n_active: 3,
      n_flags2: 1,
      expiring_week: 0,
      min_flags: 2,
      week_days: 7,
      flag_labels: { no_contact: '接触の記録が無い', expiring60: '満了まで60日以内', nps_low: '満足度が4以下' },
    },
  },
  consultants: {
    rows: [
      { consultant: '架空担当あお', contact_rate: 35, contact_touched: 2, contact_months: 6, small_n: true, focus: 1 },
      { consultant: '架空担当みどり', contact_rate: 80, contact_touched: 8, contact_months: 10, small_n: false, focus: 0 },
    ],
  },
  contact: {
    meta: { min_deals: 1 },
    month: {
      periods: [{ label: '2026-09' }],
      rows: [
        { consultant: '架空担当あお', cells: [{ deals: 1, contacts: 1, avg: 1, small_n: true }] },
        { consultant: '架空担当みどり', cells: [{ deals: 2, contacts: 3, avg: 1.5, small_n: false }] },
      ],
    },
  },
  comparison: { data_day: '2026-10-30', totals: { n_flags2: 1, expiring_week: -1 } },
};

export const DETAIL: DetailResponse = {
  meta: { today: '2026-11-02', hubspot_portal_id: '12345', found: true, deal_id: DEAL_A.deal_id, summary_sheet: 'ok' },
  deal: {
    deal_id: DEAL_A.deal_id,
    name: '架空株式会社A 本社',
    site: '本社',
    stage: '稼働中',
    start: '2026-06-01',
    expiration: '2026-11-30',
    period: 6,
    consultant: '架空担当あお',
    amount: 1_200_000,
    renewal_no: 0,
    is_active: true,
    right_censored: false,
    flags: DEAL_A.flags ?? [],
    houjin_name: '架空株式会社A',
  },
  chain: {
    rows: [
      { deal_id: '70000000009', name: '架空株式会社A 本社（前の契約）', stage: '終了', start: '2025-12-01', expiration: '2026-05-31', renewal_no: 0, amount: 900_000, is_active: false },
      { deal_id: DEAL_A.deal_id, name: '架空株式会社A 本社', stage: '稼働中', start: '2026-06-01', expiration: '2026-11-30', renewal_no: 1, amount: 1_200_000, is_active: true, current: true },
    ],
  },
  counts: {},
  events: [
    { kind: 'call', date: '2026-09-22', time: '10:30', source_label: 'Zoom Phone', duration_sec: 192, contact: true, direction: 'outbound', handler_label: '架空担当あお', has_transcript: true, summary: { summary: '架空の要約。採用の進み具合を確認した。', next_action: '面接日程を相談する', concern: '応募が増えない' } },
    { kind: 'call', date: '2026-09-20', time: '09:00', source_label: 'Zoom Phone', duration_sec: 20, contact: false, direction: 'outbound' },
    { kind: 'mtg', date: '2026-08-21', time: '14:00', source_label: 'Zoom 録画（事実）', minutes: 45, host: '架空担当あお', extracted: true, todo: '求人原稿を見直す', concern: '掲載の反応が少ない' },
    { kind: 'email', date: '2026-08-10', direction: '送信', mail_type: '応募の報告', subject: '架空の応募報告' },
    { kind: 'email', date: '2026-07-15', direction: '送信', mail_type: '案内', subject: '架空の案内メール' },
    { kind: 'handover', date: '2026-07-01', source_label: '担当履歴', from: '架空担当みどり', to: '架空担当あお' },
  ],
};

export const GOALS: GoalsResponse = {
  selected: {
    deal: {
      shokai_mtg_mokuhyou_risou: '応募を月10件にする',
      shokai_mtg_mokuhyou_zenshin: '面接を月2件入れる',
    },
  },
};

export const RENEWAL_PIPE: RenewalPipeResponse = {
  meta: { today: '2026-11-02', hubspot_portal_id: '12345', n_active: 3, window: ['2026-11', '2026-12', '2027-01'], amount_basis: '金額は契約期間全体の額（月額ではない）' },
  months: [
    { month: '2026-11', n: 1, amount: 1_200_000, amount_missing: 0, stages: [{ label: '稼働中', n: 1 }] },
    { month: '2026-12', n: 0, amount: null, amount_missing: 0, stages: [] },
    { month: '2027-01', n: 0, amount: null, amount_missing: 0, stages: [] },
  ],
  stages: ['稼働中'],
  window_total: { n: 1, amount: 1_200_000, amount_missing: 0 },
  active_total: { n: 3, amount: 2_000_000, amount_missing: 1 },
  overdue_before: { sum: { n: 1, amount: null, amount_missing: 1 }, rows: [{ deal_id: '70000000003', name: '架空株式会社C 支店', expiry: '2026-10-21', days_left: -12, stage: '稼働中', amount: null, consultant: '架空担当みどり', flags: [] }] },
  later: 1,
  no_expiry: 0,
  rows: [{ deal_id: '70000000001', name: '架空株式会社A 本社', expiry: '2026-11-30', days_left: 28, stage: '稼働中', amount: 1_200_000, consultant: '架空担当あお', flags: ['満了まで60日以内'] }],
};

export const CUSTOMER_INDEX: CustomerResponse = {
  meta: { today: '2026-11-02' },
  default_houjin: '9000000000001',
  index: [
    { houjin: '9000000000001', name: '架空株式会社A', ltv: 2_100_000, deals: 2, sites: 1, kigyouzentaininzuu: 1200, max_renewal_no: 1, active: 1, last_expiration: '2026-11-30', focus: true },
    { houjin: '9000000000002', name: '架空株式会社B', ltv: 800_000, deals: 1, sites: 1, max_renewal_no: 0, active: 1, last_expiration: '2027-03-01' },
  ],
};

export const CUSTOMER: CustomerResponse = {
  meta: { today: '2026-11-02', hubspot_portal_id: '12345', houjin: '9000000000001', found: true },
  customer: { houjin: '9000000000001', name: '架空株式会社A', ltv: 2_100_000, deals: 2, sites: 1, max_renewal_no: 1, active: 1, last_expiration: '2026-11-30', focus: true, focus_why: ['満足度が低い'] },
  deals: [
    { deal_id: '70000000009', name: '架空株式会社A 本社（前の契約）', stage: '終了', start: '2025-12-01', expiration: '2026-05-31', renewal_no: 0, amount: 900_000, oubo: 8, mensetu: 2, syoudaku: 1, is_active: false, site_name: '本社' },
    { deal_id: '70000000001', name: '架空株式会社A 本社', stage: '稼働中', start: '2026-06-01', expiration: '2026-11-30', renewal_no: 1, amount: 1_200_000, oubo: 12, mensetu: 3, syoudaku: 1, is_active: true, site_name: '本社' },
  ],
  funnel: { oubo: 20, mensetu: 5, syoudaku: 2 },
  mtgs: [{ deal_id: '70000000001', date: '2026-08-21', risk: '注意', todo: '求人原稿を見直す', concern: '掲載の反応が少ない', extracted: true }],
};

export const RESULTS: ResultsResponse = {
  meta: { today: '2026-11-02', hubspot_portal_id: '12345' },
  renewal: {
    monthly_retention: { rows: [
      { month: '2026-09', keep: 6, cancel: 2, fill: 1, denom: 9, pending: 0, rate: 66.7, rate_excl_fill: 75 },
      { month: '2026-10', keep: 1, cancel: 0, fill: 0, denom: 1, pending: 3, rate: 100, rate_excl_fill: 100 },
    ] },
    by_renewal: [{ renewal_no: 0, n: 10, n_active: 2, keep: 6, cancel: 2, fill: 1, pending: 1, denom: 9, cancel_rate: 33.3, cancel_rate_excl_fill: 25 }],
  },
  outcome: { goal_act: { pop: 3, has_goal: 2, both: 2, median: 0.25, bands: [{ label: '100%以上', n: 1 }, { label: '50〜100%', n: 0 }, { label: '1〜50%', n: 1 }] } },
  money: {
    active_total: { n: 3, amount: 2_000_000, amount_missing: 1 },
    retention: { rows: [{ month: '2026-09', keep: 4_000_000, cancel: 1_000_000, fill: 500_000, denom: 5_500_000, rate: 72.7, rate_excl_fill: 80 }] },
  },
  handover: { meta: { n: 1, median_gap_days: 2 }, rows: [{ deal_id: '70000000001', name: '架空株式会社A 本社', date: '2026-07-01', from_label: '架空担当みどり', to_label: '架空担当あお', reflected: '反映済み' }] },
  pending_recordings: { rows: [{ date: '2026-10-01' }] },
  headquarters: { meta: { cancel_rule: '解約率 ＝（解約＋充足）÷ 結果が出た件数' }, rows: [{ houjin: '9000000000001', name: '架空株式会社A', sites: 1, deals: 2, active: 1 }] },
};

export const MTGQ: MtgQualityResponse = {
  meta: { n_mtg: 10 },
  linked: { n: 8, rate: 80, note: '取引に紐づいた MTG' },
  filled: [{ field: 'やること', n: 6, rate: 60 }],
  filled_note: '埋まっていないのは記録していないのではなく、まだ抽出を通していないものです',
  risk_dist: [{ label: '注意', n: 2 }],
};

export const PHONE: PhoneResponse = {
  meta: { n_active: 3, threshold_sec: 60 },
  reach: { rows: 20, deals_with_call: 2, no_call: 1, no_contact: 1, no_call_rate: 33.3, no_contact_rate: 33.3, note: '1本の通話が複数の取引に結び付いていることがあります' },
  monthly: [{ month: '2026-10', calls: 10, contacts: 6, contact_rate: 60 }],
  silent: { n: 1, rule: '接触が1本も無い、または最後の接触から90日超', rows: [{ deal_id: '70000000002', name: '架空株式会社B 第二拠点', stage: '稼働中', n_calls: 0, n_contact: 0, last_contact: null }] },
};
