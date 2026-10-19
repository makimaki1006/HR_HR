export interface CustomerIndex {
  houjin: string;
  name: string;
  ltv: number | null;
  deals: number | null;
  sites: number | null;
  active: number | null;
  kigyouzentaininzuu: number | null;
  monthly_amount_max: number | null;
  max_renewal_no: number | null;
  oubo_total: number | null;
  mensetu_total: number | null;
  syoudaku_total: number | null;
  last_expiration: string;
  focus: boolean;
  focus_why: string[];
}
export interface CustomerDeal {
  deal_id: string;
  name: string;
  stage: string;
  kind: string;
  start: string;
  expiration: string;
  renewal_no: number | null;
  amount: number | null;
  oubo: number | null;
  mensetu: number | null;
  syoudaku: number | null;
  is_active: boolean;
  right_censored: boolean;
  site: string;
  site_name: string;
}
export interface Point {
  m: number;
  v: number | null;
  carry?: boolean;
}
export interface Monthly {
  deal_id: string;
  name: string;
  start: string;
  expiration: string;
  until: string;
  period: number | null;
  span_months: number | null;
  std_expiration: string | null;
  series: Record<string, Point[]>;
  nps: Record<string, Point[]>;
}
export interface CustomerData {
  meta: {
    today: string;
    found: boolean;
    houjin: string;
    hubspot_portal_id?: string;
    source_as_of?: string;
    source_age_days?: number;
  };
  customer: {
    name: string;
    ltv: number | null;
    deals: number | null;
    sites: number | null;
    active: number | null;
    max_renewal_no: number | null;
    last_expiration: string;
    focus: boolean;
    focus_why: string[];
  } | null;
  deals: CustomerDeal[];
  monthly: Monthly[];
  cpa3: {
    deal_id: string;
    name: string;
    start: string;
    total: number | null;
    monthly: number | null;
    band: string | null;
    band_median: number | null;
    syoudaku: number | null;
    censored: boolean;
    active: boolean;
  }[];
  cpa_by_site: {
    site: string;
    points: {
      deal_id: string;
      start: string;
      cpa: number;
      syoudaku: number | null;
      amount: number | null;
      renewal_no: number | null;
      censored: boolean;
      active: boolean;
    }[];
  }[];
  contacts: { deal_id: string; dates: string[] }[];
  handover: {
    deal_id: string;
    date: string;
    from: string;
    to: string;
    reflected: string;
  }[];
  mtgs: {
    deal_id: string;
    date: string;
    risk: string;
    todo: string;
    concern: string;
    positive: string;
    next: string;
    extracted: boolean;
  }[];
  focus: {
    n_display: number;
    n_focus: number;
    n_focus_all: number;
    n_houjin: number;
    monthly_over_300k: number;
    enterprise: number;
    multi_site: number;
    n_display_option_only: number;
    n_houjin_option_only: number;
    rule: string;
  };
}
export const customerFilters = [
  ["employees", "kigyouzentaininzuu", "従業員数", "人", 1],
  ["monthly", "monthly_amount_max", "月額", "万円", 10000],
  ["sites", "sites", "拠点数", "拠点", 1],
  ["ltv", "ltv", "これまでの売上（LTV）", "万円", 10000],
  ["renewals", "max_renewal_no", "継続の回数", "回", 1],
  ["applications", "oubo_total", "応募", "件", 1],
  ["interviews", "mensetu_total", "面接", "件", 1],
  ["acceptances", "syoudaku_total", "承諾", "件", 1],
] as const;
export const customerPresets: {
  label: string;
  values: Record<string, string>;
}[] = [
  { label: "本社に提案できそう", values: { acceptances: "5", active: "1" } },
  {
    label: "大きいのに拠点が少ない",
    values: { employees: "1000", sites_max: "2" },
  },
  { label: "お金を多くかけている", values: { monthly: "30", active: "1" } },
  { label: "重点顧客", values: { focus: "1" } },
];
export const filterKeys = [
  ...customerFilters.map(([key]) => key),
  "sites_max",
  "active",
  "focus",
];
export function presetQuery(
  values: Record<string, string>,
): Record<string, string | null> {
  return Object.fromEntries(
    filterKeys.map((key) => [`houjin_${key}`, values[key] ?? null]),
  );
}
export function filterCustomers(rows: CustomerIndex[], q: URLSearchParams) {
  const min = (key: string) => {
    const raw = q.get(`houjin_${key}`);
    return raw !== null &&
      raw.trim() !== "" &&
      Number.isFinite(Number(raw)) &&
      Number(raw) >= 0
      ? Number(raw)
      : null;
  };
  const text = q.get("houjin_search")?.trim().toLocaleLowerCase() ?? "";
  const filtered = rows.filter(
    (r) =>
      (!text || `${r.name} ${r.houjin}`.toLocaleLowerCase().includes(text)) &&
      (q.get("houjin_focus") !== "1" || r.focus) &&
      (q.get("houjin_active") !== "1" || (r.active ?? 0) > 0) &&
      customerFilters.every(([key, field, , , scale]) => {
        const n = min(key);
        return n === null || (r[field] !== null && r[field] >= n * scale);
      }) &&
      (min("sites_max") === null ||
        (r.sites !== null && r.sites <= (min("sites_max") ?? 0))),
  );
  return filtered.sort((a, b) =>
    q.get("houjin_sort") === "name"
      ? a.name.localeCompare(b.name, "ja")
      : q.get("houjin_sort") === "deals"
        ? (b.deals ?? -Infinity) - (a.deals ?? -Infinity)
        : (b.ltv ?? -Infinity) - (a.ltv ?? -Infinity),
  );
}
export const siteKey = (d: CustomerDeal) => d.site || "__no_site__";
export function sumKnown<T>(
  rows: T[],
  value: (r: T) => number | null,
): number | null {
  const values = rows.map(value).filter((v): v is number => v !== null);
  return values.length ? values.reduce((a, b) => a + b, 0) : null;
}
export function sitesOf(deals: CustomerDeal[]) {
  const keys = [...new Set(deals.map(siteKey))];
  return keys
    .map((key) => {
      const rows = deals.filter((d) => siteKey(d) === key);
      return {
        key,
        name:
          rows.find((d) => d.site_name)?.site_name ??
          (key === "__no_site__" ? "拠点未登録" : key),
        rows,
        lastStart: rows.reduce((m, d) => (d.start > m ? d.start : m), ""),
      };
    })
    .sort(
      (a, b) =>
        b.rows.length - a.rows.length ||
        b.lastStart.localeCompare(a.lastStart) ||
        a.key.localeCompare(b.key),
    );
}
export function monthAdd(start: string, m: number) {
  if (!/^\d{4}-\d{2}/.test(start)) return "";
  const n =
    Number(start.slice(0, 4)) * 12 + Number(start.slice(5, 7)) - 1 + m - 1;
  return `${String(Math.floor(n / 12))}-${String((n % 12) + 1).padStart(2, "0")}`;
}
export function hireAt(mm: Monthly, month: string): Point | null {
  if (
    month < mm.start.slice(0, 7) ||
    month > (mm.expiration || "9999-12").slice(0, 7)
  )
    return null;
  const points = mm.series.syoudaku ?? [];
  const exact = points.find((p) => monthAdd(mm.start, p.m) === month);
  if (exact) return exact;
  const prev = points.filter((p) => monthAdd(mm.start, p.m) < month).at(-1);
  return prev ? { ...prev, carry: true } : null;
}
