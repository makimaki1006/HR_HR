import { useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CallQueuePipelinesResponse } from '../../generated/CallQueuePipelinesResponse';
import { pipelinesFor, withLabels } from './queuePipelines';
import type { QueuePipelineDef } from './queuePipelines';
import type { QueueMode } from './queueModel';

export type PipelinesFetch = (signal: AbortSignal) => Promise<ApiResult<CallQueuePipelinesResponse>>;

export const livePipelinesFetch: PipelinesFetch = signal =>
  apiGet<CallQueuePipelinesResponse>('/api/crm/call-queue/pipelines', { signal, timeoutMs: 30_000 });

export interface QueuePipelinesState {
  pipelines: readonly QueuePipelineDef[];
  /** 実データで HubSpot から名前を読めなかった (表の呼び名で出している) */
  labelsUnavailable: boolean;
}

/**
 * 選べるパイプラインとステージ名。
 * - 架空サンプル: 固定 (HubSpot に接続しない)
 * - 実データ: ID と決まりは表 (生成 JSON) で最初から使え、名前は GET /api/crm/call-queue/pipelines で 1 回読んで足す。
 *   読めないときも選択肢は出す (名前は表の呼び名・「名称を取得できません」)
 */
export function useQueuePipelines(mode: QueueMode, fetcher: PipelinesFetch = livePipelinesFetch): QueuePipelinesState {
  const [live, setLive] = useState<{ resp: CallQueuePipelinesResponse | null; failed: boolean }>({ resp: null, failed: false });
  const needLive = mode === 'live' && live.resp === null && !live.failed;
  useEffect(() => {
    if (!needLive) return;
    const ctl = new AbortController();
    void fetcher(ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      if (!r.ok) { setLive({ resp: null, failed: true }); return; }
      setLive({ resp: r.data, failed: !r.data.labels_available });
    });
    return () => { ctl.abort(); };
  }, [needLive, fetcher]);
  const labelled = useMemo(() => withLabels(pipelinesFor('live'), live.resp), [live.resp]);
  if (mode === 'fixture') return { pipelines: pipelinesFor('fixture'), labelsUnavailable: false };
  return { pipelines: labelled, labelsUnavailable: live.failed };
}
