// Minimal query-string router shared by the admin and my screens (W8).
// TODO(platform-team): replace with the App Shell router once it exists.
import { useCallback, useEffect, useState } from 'react';

function currentSearch(): string {
  return typeof window === 'undefined' ? '' : window.location.search;
}

/**
 * `parse` and `toHref` must be module-level (stable) functions.
 * Returns the current route and a `navigate` that pushes history without a reload.
 */
export function useQueryRoute<R>(
  parse: (search: string) => R,
  toHref: (route: R) => string,
): [R, (route: R) => void] {
  const [route, setRoute] = useState<R>(() => parse(currentSearch()));

  useEffect(() => {
    const onPopState = (): void => {
      setRoute(parse(currentSearch()));
    };
    window.addEventListener('popstate', onPopState);
    return () => {
      window.removeEventListener('popstate', onPopState);
    };
  }, [parse]);

  const navigate = useCallback(
    (next: R): void => {
      window.history.pushState(null, '', toHref(next));
      setRoute(next);
    },
    [toHref],
  );

  return [route, navigate];
}
