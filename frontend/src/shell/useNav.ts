import { useEffect, useState } from 'react';
import { AuthRequiredError, apiGet } from '../api/client';
import { redirectToLogin } from './navigation';
import type { NavResponse } from './types';

/** Fetches GET /api/nav. Failure leaves `nav` null (the shell renders without a nav). */
export function useNav(): NavResponse | null {
  const [nav, setNav] = useState<NavResponse | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    void apiGet<NavResponse>('/api/nav', { signal: controller.signal }).then((r) => {
      if (r.ok) setNav(r.data);
      else if (r.error instanceof AuthRequiredError) redirectToLogin();
    });
    return () => {
      controller.abort();
    };
  }, []);
  return nav;
}
