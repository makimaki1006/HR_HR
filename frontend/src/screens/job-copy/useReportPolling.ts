import { useEffect } from 'react';

/** Reports advance independently, and hidden tabs do not send repeated reads. */
export function useReportPolling(load: (signal: AbortSignal) => Promise<boolean>, enabled = true, reload = 0) {
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let delay = 2_000;
    let pending = true;
    let loading = false;
    const schedule = () => {
      if (!pending || loading || document.hidden || controller.signal.aborted) return;
      timer = setTimeout(() => { delay = Math.min(delay * 2, 30_000); void run(); }, delay);
    };
    const run = async () => {
      if (document.hidden || controller.signal.aborted) return;
      loading = true;
      try { pending = await load(controller.signal); }
      finally { loading = false; }
      schedule();
    };
    const visibility = () => {
      clearTimeout(timer);
      if (!document.hidden) schedule();
    };
    document.addEventListener('visibilitychange', visibility);
    void run();
    return () => {
      controller.abort(); clearTimeout(timer);
      document.removeEventListener('visibilitychange', visibility);
    };
  }, [load, enabled, reload]);
}
