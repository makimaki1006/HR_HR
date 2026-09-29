// Hand-written mirrors of the Rust contract for GET /api/nav and GET /api/filters/current.
// TODO: replace with the ts-rs generated NavResponse.ts / FiltersCurrent.ts once the Rust side lands
// (src/generated/), then delete this file.

export type NavKind = 'legacy_tab' | 'page' | 'app';

export interface NavItem {
  id: string;
  label: string;
  title: string | null;
  kind: NavKind;
  href: string;
  group: string | null;
  /** true = registered but not shown in the nav (screen stays reachable by URL). */
  hidden: boolean;
  hidden_reason: string | null;
  hidden_since: string | null;
}

export interface NavGroup {
  id: string;
  label: string;
}

export interface NavResponse {
  user_email: string;
  is_admin: boolean;
  header_links: NavItem[];
  items: NavItem[];
  groups: NavGroup[];
}

/** Unselected is "" / []. */
export interface FiltersCurrent {
  prefecture: string;
  municipality: string;
  job_types: string[];
  industry_raws: string[];
}
