// In-screen routing for /app/admin (W8). The Rust shell only routes /app/{screen},
// so the four admin views are told apart by the query string:
//   /app/admin                     -> usage (30 days)   (old: /admin/usage)
//   /app/admin?view=usage&days=7   -> usage (7 days)    (old: /admin/usage?days=7)
//   /app/admin?view=users          -> users list        (old: /admin/users)
//   /app/admin?view=user&id=<aid>  -> user detail       (old: /admin/users/{aid})
//   /app/admin?view=login-failures -> login failures    (old: /admin/login-failures)
//   /app/admin?view=hubspot        -> HubSpot usage     (new 2026-10-08, no old page)
//   /app/admin?view=callers        -> today's callers   (new 2026-10-13; add &mode=fixture for fake data)
//   /app/admin?view=crm-failed     -> HubSpot に保存できなかった操作 (2026-10-25)
//   /app/admin?view=talk&from=&to=&team=&owner=&call=&tab= -> トークの振り返り (ADR-033; add &mode=fixture for fake data)

export type AdminRoute =
  | { view: 'usage'; days: number }
  | { view: 'users' }
  | { view: 'user'; id: string }
  | { view: 'login-failures' }
  | { view: 'hubspot' }
  | { view: 'callers' }
  | { view: 'crm-failed' }
  | { view: 'talk'; from: string; to: string; team: string; owner: string; call: string; people: string; only: string; cmp_from: string; cmp_to: string; vs_team: string; tab: string };

/** Same default as the Rust handler (`q.days.unwrap_or(30)`). */
export const DEFAULT_USAGE_DAYS = 30;

export function parseAdminRoute(search: string): AdminRoute {
  const q = new URLSearchParams(search);
  switch (q.get('view')) {
    case 'users':
      return { view: 'users' };
    case 'user': {
      const id = q.get('id') ?? '';
      return id === '' ? { view: 'users' } : { view: 'user', id };
    }
    case 'login-failures':
      return { view: 'login-failures' };
    case 'hubspot':
      return { view: 'hubspot' };
    case 'callers':
      return { view: 'callers' };
    case 'crm-failed':
      return { view: 'crm-failed' };
    case 'talk':
      return {
        view: 'talk',
        from: q.get('from') ?? '',
        to: q.get('to') ?? '',
        team: q.get('team') ?? '',
        owner: q.get('owner') ?? '',
        call: q.get('call') ?? '',
        people: q.get('people') ?? '',
        only: q.get('only') ?? '',
        cmp_from: q.get('cmp_from') ?? '',
        cmp_to: q.get('cmp_to') ?? '',
        vs_team: q.get('vs_team') ?? '',
        // 人ごとの表のタブ (空 = 数字、talk = トーク)
        tab: q.get('tab') ?? '',
      };
    default: {
      // Rust clamps to 1..=365 server-side; the response's `days` is what the UI shows.
      const days = Number.parseInt(q.get('days') ?? '', 10);
      return { view: 'usage', days: Number.isFinite(days) && days > 0 ? days : DEFAULT_USAGE_DAYS };
    }
  }
}

export function adminHref(route: AdminRoute): string {
  const q = new URLSearchParams();
  q.set('view', route.view);
  if (route.view === 'user') q.set('id', route.id);
  if (route.view === 'usage') q.set('days', String(route.days));
  if (route.view === 'talk') {
    for (const k of ['from', 'to', 'team', 'owner', 'call', 'people', 'only', 'cmp_from', 'cmp_to', 'vs_team', 'tab'] as const) {
      if (route[k] !== '') q.set(k, route[k]);
    }
    // 架空サンプルの表示は、画面を移っても保つ
    if (typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('mode') === 'fixture') q.set('mode', 'fixture');
  }
  return `?${q.toString()}`;
}

/** JSON endpoint behind each view (all under require_admin in Rust). */
export function adminApiPath(route: AdminRoute): string {
  switch (route.view) {
    case 'usage':
      return `/api/admin/usage?days=${String(route.days)}`;
    case 'users':
      return '/api/admin/users';
    case 'user':
      return `/api/admin/users/${encodeURIComponent(route.id)}`;
    case 'login-failures':
      return '/api/admin/login-failures';
    case 'hubspot':
      return '/api/admin/hubspot-usage';
    case 'callers':
      return '/api/crm/metrics/today/all';
    case 'crm-failed':
      return '/api/admin/crm-operations?status=failed';
    case 'talk':
      return '/api/crm/talk-review/summary';
  }
}
