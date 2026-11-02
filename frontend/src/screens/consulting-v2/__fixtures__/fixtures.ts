// 画面のテスト・スクリーンショット用の架空データ。実在の会社・人・取引 ID は入れない。
// 形は /api/consulting/* の応答 (src/handlers/cs_dashboard/routes.rs) に合わせてある。
import type { BoardResponse, DealRow, DetailResponse, GoalsResponse, TeamResponse, TodayResponse } from '../types';

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
  meta: { today: '2026-11-02', n_hit: 2, n_shown: 2, n_flags2: 1, filter_rule: '候補は 2 件です（架空の説明文）。' },
  rows: [DEAL_A, DEAL_B],
  candidates: [DEAL_A, DEAL_B],
  expiring_this_week: [],
  started_this_week: [DEAL_B],
  not_started: [],
  consultants: ['架空担当あお', '架空担当みどり'],
};

export const TEAM: TeamResponse = {
  meta: { n_consultant: 2, n_active: 3, retired_deals: 0, retired_people: 0, unknown_owner: 0 },
  status: {
    rows: [
      { consultant: '架空担当あお', n_active: 1, unattended: 1, n_flags2: 1, mtg_critical: 1, no_contact: 0, expiring60: 1, nps_low: 1, expiring_week: 0, delta: { n_flags2: 1 } },
      { consultant: '架空担当みどり', n_active: 2, unattended: 1, n_flags2: 0, mtg_critical: 0, no_contact: 1, expiring60: 0, nps_low: 0, expiring_week: 0 },
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
    month: {
      periods: [{ label: '2026-09' }],
      rows: [
        { consultant: '架空担当あお', cells: [{ deals: 1, contacts: 1, avg: 1, small_n: true }] },
        { consultant: '架空担当みどり', cells: [{ deals: 2, contacts: 3, avg: 1.5, small_n: false }] },
      ],
    },
  },
  comparison: { data_day: '2026-10-30', totals: { n_flags2: 1 } },
};

export const DETAIL: DetailResponse = {
  meta: { today: '2026-11-02', found: true, deal_id: DEAL_A.deal_id, summary_sheet: 'ok' },
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
