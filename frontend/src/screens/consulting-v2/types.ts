// /api/consulting/* の応答の形（src/handlers/cs_dashboard/routes.rs が正本）。
// この画面で使う項目だけを書く。値が無いとき null か項目ごと無いことがあるので、どれも省略可で扱う。

export interface DealRow {
  deal_id: string;
  name?: string | null;
  stage?: string | null;
  consultant?: string | null;
  retired?: boolean;
  amount?: number | null;
  focus?: boolean;
  flags?: string[];
  n_flags?: number;
  phase?: number | null;
  phase_key?: string | null;
  phase_label?: string | null;
  past_expiry?: boolean;
  not_started?: boolean;
  start?: string | null;
  period?: number | null;
  months?: number | null;
  band?: string | null;
  days_left?: number | null;
  nps?: number | null;
  nps_month?: string | null;
  n_contact?: number | null;
  days_since_contact?: number | null;
  mtg_band?: string | null;
  mtg_days?: number | null;
  mtg_source?: string | null;
  mtg_source_label?: string | null;
  mtg_forced_by_expiry?: boolean;
  overdue_active?: boolean;
  expiry?: string | null;
  oubo?: number | null;
  oubo_carry?: boolean | null;
  mensetu?: number | null;
  mensetu_carry?: boolean | null;
  syoudaku?: number | null;
  syoudaku_carry?: boolean | null;
  saiyomokuhyou?: number | null;
  rate_tassei?: number | null;
  contact_touched?: number | null;
  contact_months?: number | null;
  contact_rate?: number | null;
  cpa?: number | null;
  cpa_band?: string | null;
  cpa_band_median?: number | null;
  cpa_vs_band?: number | null;
}

export interface BoardMeta {
  today?: string;
  hubspot_portal_id?: string;
  n_active?: number;
  flag_counts?: { label: string; n: number }[];
  generated_at?: string | null;
  source_as_of?: string | null;
  mtg_gap?: { bands?: { band: string; label: string; n: number; alert: boolean }[] };
}

export interface BoardResponse {
  meta: BoardMeta;
  rows: DealRow[];
}

export interface TodayMeta extends BoardMeta {
  n_hit?: number;
  n_shown?: number;
  n_flags2?: number;
  filter_rule?: string;
  keep?: number;
}

export interface TodayResponse {
  meta: TodayMeta;
  rows: DealRow[];
  candidates?: DealRow[];
  expiring_this_week?: DealRow[];
  started_this_week?: DealRow[];
  not_started?: DealRow[];
  consultants?: string[];
}

export interface TeamStatusRow {
  consultant: string;
  retired?: boolean;
  n_active: number;
  unattended?: number;
  n_flags2: number;
  mtg_critical: number;
  no_contact: number;
  expiring60: number;
  nps_low: number;
  expiring_week: number;
  delta?: Partial<Record<string, number>>;
}

export interface ConsultantRow {
  consultant: string;
  contact_rate?: number | null;
  contact_touched?: number | null;
  contact_months?: number | null;
  small_n?: boolean;
  focus?: number | null;
}

export interface ContactCell {
  deals?: number;
  contacts?: number;
  avg?: number | null;
  small_n?: boolean;
}

export interface ContactPeriod {
  label: string;
  provisional?: boolean;
  calls_missing?: boolean;
}

export interface ContactUnit {
  periods?: ContactPeriod[];
  rows?: { consultant: string; cells: (ContactCell | null)[] }[];
  team?: (ContactCell | null)[];
}

export interface TeamResponse {
  meta: {
    hubspot_portal_id?: string;
    n_consultant?: number;
    n_active?: number;
    retired_deals?: number;
    retired_people?: number;
    unknown_owner?: number;
    not_counted?: string;
    source_as_of?: string | null;
  };
  status: {
    rows: TeamStatusRow[];
    meta: {
      n_active?: number;
      n_flags2?: number;
      expiring_week?: number;
      min_flags?: number;
      week_days?: number;
      unknown?: Partial<Record<string, number>>;
      flag_labels?: Record<string, string>;
    };
  };
  consultants?: { rows?: ConsultantRow[] };
  contact?: {
    meta?: { min_deals?: number };
    month?: ContactUnit;
    week?: ContactUnit;
  };
  comparison?: { data_day?: string; totals?: Partial<Record<string, number>> };
}

export interface DetailAttach {
  state?: string;
  moved_from?: { deal_id?: string; name?: string; relation?: string } | null;
  moved_to?: { deal_id?: string; name?: string } | null;
}

export interface DetailEvent {
  kind: string;
  date: string;
  time?: string | null;
  end_time?: string | null;
  source_label?: string | null;
  minutes?: number | null;
  host?: string | null;
  mtg_type?: string | null;
  extracted?: boolean;
  todo?: string | null;
  concern?: string | null;
  positive?: string | null;
  risk?: string | null;
  risk_reason?: string | null;
  next?: string | null;
  link_certainty?: string | null;
  link_reason?: string | null;
  direction?: string | null;
  mail_type?: string | null;
  subject?: string | null;
  duration_sec?: number | null;
  contact?: boolean;
  handler?: string | null;
  handler_label?: string | null;
  owner?: string | null;
  has_transcript?: boolean;
  summary?: {
    summary?: string | null;
    next_action?: string | null;
    concern?: string | null;
    model?: string | null;
    generated_at?: string | null;
  } | null;
  from?: string | null;
  to?: string | null;
  to_retired?: boolean;
  reflected?: string | null;
  record_gap_days?: number | null;
  same_day_recording?: boolean;
  certainty?: string | null;
  attach?: DetailAttach | null;
}

export interface ChainRow {
  deal_id: string;
  name?: string | null;
  stage?: string | null;
  start?: string | null;
  expiration?: string | null;
  renewal_no?: number | null;
  amount?: number | null;
  is_active?: boolean;
  current?: boolean;
}

export interface DetailResponse {
  meta: {
    today?: string;
    hubspot_portal_id?: string;
    found?: boolean;
    deal_id?: string | null;
    reason?: string | null;
    summary_sheet?: string;
  };
  deal?: {
    deal_id: string;
    name?: string | null;
    site?: string | null;
    stage?: string | null;
    start?: string | null;
    expiration?: string | null;
    period?: number | null;
    consultant?: string | null;
    consultant_retired?: boolean;
    amount?: number | null;
    renewal_no?: number | null;
    is_active?: boolean;
    right_censored?: boolean;
    flags?: string[] | null;
    houjin_name?: string | null;
  };
  chain?: { rows?: ChainRow[] };
  counts?: Record<string, number | { kind: string; n: number }[] | undefined>;
  events?: DetailEvent[];
}

/** 案件管理の取引読み取り (/api/crm/workspace/deals/{id}) のうち、目標の 3 項目だけ */
export interface GoalsResponse {
  partial?: { part: string }[];
  selected?: { deal?: Record<string, string | null | undefined> };
}

export interface PipeAmount {
  n: number;
  amount: number | null;
  amount_missing: number;
}

export interface PipeRow {
  deal_id: string;
  name?: string | null;
  expiry?: string | null;
  days_left?: number | null;
  stage?: string | null;
  amount?: number | null;
  consultant?: string | null;
  flags?: string[];
}

export interface RenewalPipeResponse {
  meta: { today?: string; hubspot_portal_id?: string; n_active?: number; window?: string[]; amount_basis?: string };
  months?: (PipeAmount & { month: string; stages?: { label: string; n: number }[] })[];
  stages?: string[];
  window_total?: PipeAmount;
  active_total?: PipeAmount;
  overdue_before?: { sum: PipeAmount; rows: PipeRow[] };
  later?: number;
  no_expiry?: number;
  rows?: PipeRow[];
}

export interface CustomerIndexRow {
  houjin: string;
  name?: string | null;
  ltv?: number | null;
  deals?: number;
  sites?: number;
  kigyouzentaininzuu?: number | null;
  max_renewal_no?: number | null;
  active?: number;
  last_expiration?: string | null;
  focus?: boolean;
  focus_why?: string[];
}

export interface CustomerDeal {
  deal_id: string;
  name?: string | null;
  stage?: string | null;
  kind?: string | null;
  start?: string | null;
  expiration?: string | null;
  renewal_no?: number | null;
  amount?: number | null;
  oubo?: number | null;
  mensetu?: number | null;
  syoudaku?: number | null;
  is_active?: boolean;
  right_censored?: boolean;
  site?: string | null;
  site_name?: string | null;
}

export interface CustomerResponse {
  meta: { today?: string; hubspot_portal_id?: string; houjin?: string; found?: boolean };
  default_houjin?: string | null;
  index?: CustomerIndexRow[];
  customer?: CustomerIndexRow | null;
  deals?: CustomerDeal[];
  funnel?: { oubo?: number | null; mensetu?: number | null; syoudaku?: number | null };
  mtgs?: { deal_id: string; date?: string; risk?: string; todo?: string; concern?: string; extracted?: boolean }[];
}

export interface MonthlyRetentionRow {
  month: string;
  keep: number;
  cancel: number;
  fill: number;
  denom: number;
  pending: number;
  rate: number | null;
  rate_excl_fill: number | null;
}

export interface ByRenewalRow {
  renewal_no: number;
  n: number;
  n_active: number;
  keep: number;
  cancel: number;
  fill: number;
  pending: number;
  denom: number;
  cancel_rate: number | null;
  cancel_rate_excl_fill: number | null;
}

export interface MoneyRetentionRow {
  month: string;
  keep: number | null;
  cancel: number | null;
  fill: number | null;
  denom: number | null;
  rate: number | null;
  rate_excl_fill: number | null;
}

export interface ResultsResponse {
  meta: { today?: string; hubspot_portal_id?: string };
  renewal?: { monthly_retention?: { rows?: MonthlyRetentionRow[] }; by_renewal?: ByRenewalRow[] };
  outcome?: { goal_act?: { pop?: number; has_goal?: number; both?: number; median?: number | null; bands?: { label: string; n: number }[] } };
  money?: { active_total?: PipeAmount; window?: { months?: string[]; sum?: PipeAmount }; retention?: { rows?: MoneyRetentionRow[]; rule?: string }; amount_basis?: string };
  handover?: {
    meta?: { n?: number; n_active?: number; median_gap_days?: number | null; gap_rule?: string };
    rows?: { deal_id: string; name?: string; date?: string; from_label?: string; to_label?: string; to_retired?: boolean; reflected?: string; is_active?: boolean }[];
  };
  pending_recordings?: { rows?: { date: string; subject?: string; host?: string; candidates?: string; reason?: string }[] } | null;
  headquarters?: {
    meta?: { n_houjin?: number; cpa_rule?: string; cancel_rule?: string };
    rows?: { houjin: string; name?: string | null; sites: number; deals: number; active: number; spread?: number | null }[];
  };
}

export interface MtgQualityResponse {
  meta: { n_mtg?: number };
  linked?: { n: number; rate: number | null; note?: string };
  filled?: { field: string; n: number; rate: number | null }[];
  filled_note?: string;
  risk_dist?: { label: string; n: number }[];
  hosts?: { host: string; n: number }[];
}

export interface PhoneResponse {
  meta: { n_active?: number; threshold_sec?: number };
  reach?: { rows?: number; deals_with_call?: number; no_call?: number; no_contact?: number; no_call_rate?: number | null; no_contact_rate?: number | null; note?: string };
  silent?: { n: number; rule?: string; rows?: { deal_id: string; name?: string; stage?: string; n_calls?: number; n_contact?: number; last_contact?: string | null; days_since?: number | null }[] };
  monthly?: { month: string; calls: number; contacts: number; contact_rate: number | null }[];
}
