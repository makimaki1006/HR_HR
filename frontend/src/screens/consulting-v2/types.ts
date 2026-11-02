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
}

export interface BoardMeta {
  today?: string;
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

export interface TeamResponse {
  meta: {
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
    month?: {
      periods?: ContactPeriod[];
      rows?: { consultant: string; cells: (ContactCell | null)[] }[];
    };
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
