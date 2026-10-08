// In-screen routing for /app/admin (W8). The Rust shell only routes /app/{screen},
// so the four admin views are told apart by the query string:
//   /app/admin                     -> usage (30 days)   (old: /admin/usage)
//   /app/admin?view=usage&days=7   -> usage (7 days)    (old: /admin/usage?days=7)
//   /app/admin?view=users          -> users list        (old: /admin/users)
//   /app/admin?view=user&id=<aid>  -> user detail       (old: /admin/users/{aid})
//   /app/admin?view=login-failures -> login failures    (old: /admin/login-failures)
//   /app/admin?view=hubspot        -> HubSpot usage     (new 2026-10-08, no old page)

export type AdminRoute =
  | { view: 'usage'; days: number }
  | { view: 'users' }
  | { view: 'user'; id: string }
  | { view: 'login-failures' }
  | { view: 'hubspot' };

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
  }
}
