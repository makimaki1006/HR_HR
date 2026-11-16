import { useEffect } from 'react';
import { backoff, runWhenDue } from '../../shared/preparingRetry';

/**
 * Reports advance independently, and hidden tabs do not send repeated reads.
 * 表示に戻ったとき、前回から間隔以上たっていればすぐ 1 回読む。
 */
export function useReportPolling(load: (signal: AbortSignal) => Promise<boolean>, enabled = true, reload = 0) {
  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    const wait = backoff(2_000, 30_000);
    const run = async () => {
      if (controller.signal.aborted) return;
      const pending = await load(controller.signal);
      if (pending) runWhenDue(controller.signal, wait(), () => { void run(); });
    };
    void run();
    return () => { controller.abort(); };
  }, [load, enabled, reload]);
}
