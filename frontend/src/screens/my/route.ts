// In-screen routing for /app/my (W8):
//   /app/my                -> profile   (old: /my/profile)
//   /app/my?view=activity  -> activity  (old: /my/activity)

export type MyRoute = { view: 'profile' } | { view: 'activity' };

export function parseMyRoute(search: string): MyRoute {
  return new URLSearchParams(search).get('view') === 'activity'
    ? { view: 'activity' }
    : { view: 'profile' };
}

export function myHref(route: MyRoute): string {
  return `?view=${route.view}`;
}

export function myApiPath(route: MyRoute): string {
  return route.view === 'activity' ? '/api/my/activity' : '/api/my/profile';
}
