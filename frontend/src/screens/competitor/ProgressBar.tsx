// レポート作成中の進捗バー。サーバの段階 (n / 全体) と今の段階名を出す。
// CSV の送信中はサーバに記録がまだ無いので、送信の割合を出す。
import { useEffect, useState } from 'react';
import type { UploadProgress } from '../../api/client';
import type { CompetitorProgress } from '../../generated/CompetitorProgress';
import { fetchProgress } from './api';

/** 取り直しの間隔。タブが隠れている間は取りに行かない。 */
export const PROGRESS_POLL_MS = 1500;

/** 進捗を取り直す。`id` が null の間は止まる。サーバに記録がまだ無い間は null。 */
export function useReportProgress(id: string | null): CompetitorProgress | null {
  const [got, setGot] = useState<{ id: string; data: CompetitorProgress } | null>(null);
  useEffect(() => {
    if (id === null) return undefined;
    const controller = new AbortController();
    const tick = (): void => {
      if (document.hidden) return;
      void fetchProgress(id, controller.signal).then((res) => {
        if (!controller.signal.aborted && res.ok) setGot({ id, data: res.data });
      });
    };
    tick();
    const timer = setInterval(tick, PROGRESS_POLL_MS);
    return () => {
      clearInterval(timer);
      controller.abort();
    };
  }, [id]);
  return got !== null && got.id === id ? got.data : null;
}

export function ProgressBar({
  stage,
  upload,
}: {
  stage: CompetitorProgress | null;
  upload: UploadProgress | null;
}) {
  const uploading = stage === null;
  const max = uploading ? 100 : stage.total;
  const now = uploading
    ? Math.round((upload?.ratio ?? 0) * 100)
    : Math.min(stage.completed, stage.total);
  const label = uploading
    ? `CSVを送信中${upload !== null ? ` ${String(now)}%` : ''}`
    : `${stage.label}（${String(stage.completed)} / ${String(stage.total)}）`;
  return (
    <div className="cmp-progress">
      <div
        role="progressbar"
        aria-label="レポート作成の進み具合"
        aria-valuemin={0}
        aria-valuemax={max}
        aria-valuenow={now}
        aria-valuetext={label}
        className="cmp-progress__track"
      >
        <div className="cmp-progress__fill" style={{ width: `${String((now / max) * 100)}%` }} />
      </div>
      <p className="cmp-progress__label">{label}</p>
    </div>
  );
}
