import type { ApiResult } from '../../api/client';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { QUEUE_STAGE_IDS } from './queueModel';
import type { QueueFilters } from './queueModel';

/** 架空の 1 行 (実データ由来の値は使わない) */
export function makeItem(id: string, over: Partial<CallQueueItem> = {}): CallQueueItem {
  return {
    deal_id: id, deal_name: `架空案件${id}`, stage_id: '1095387442', stage_label: '未済', owner_id: '9001',
    next_call_date: null, next_call_time: null, last_call_date: null,
    stop: { prohibited_reason: null, block_reason: null, unreachable_check: null },
    contact: { id: `c${id}`, name: `架空 太郎${id}`, phone: '+81300000001', mobile: null, job_title: '採用担当', extra_count: 0 },
    company: { id: `co${id}`, name: `架空会社${id}`, phone: null },
    phone: '+81300000001', phone_source: 'contact',
    deep_links: { deal: `https://example.invalid/deal/${id}`, contact: null, company: null },
    ...over,
  };
}

/** 条件 f に対応する scope を持つ応答 */
export function makeResponse(f: QueueFilters, items: CallQueueItem[], over: Partial<CallQueueResponse> = {}): CallQueueResponse {
  return {
    items, next_cursor: null, total: items.length, truncated: false,
    scope: {
      owner: f.owner === '' ? 'all' : f.owner, role: 'admin', teams: [],
      stages: [...(f.stages.length ? f.stages : QUEUE_STAGE_IDS)].sort(), due: f.due, sort: f.sort,
      q: f.q.trim() || null, limit: 25,
      next_from: f.nextFrom || null, next_to: f.nextTo || null, last_from: f.lastFrom || null, last_to: f.lastTo || null,
    },
    partial: { missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 } },
    generated_at: '2026-10-05T03:00:00Z',
    ...over,
  };
}

export interface PendingCall {
  filters: QueueFilters;
  cursor: string | null;
  signal: AbortSignal;
  resolve: (r: ApiResult<CallQueueResponse>) => void;
}

/** 呼び出しを溜めておき、テストが好きな順・好きなタイミングで応答する偽の取得関数 */
export function deferredFetcher() {
  const calls: PendingCall[] = [];
  const fetcher = (filters: QueueFilters, cursor: string | null, signal: AbortSignal) =>
    new Promise<ApiResult<CallQueueResponse>>(resolve => { calls.push({ filters, cursor, signal, resolve }); });
  return { calls, fetcher };
}
