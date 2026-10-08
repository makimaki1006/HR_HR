import type { ApiResult } from '../../api/client';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueueResponse } from '../../generated/CallQueueResponse';
import { FIXTURE_PIPELINES, LIVE_PIPELINES, eligibleStageIds } from './queuePipelines';
import type { CallQueuePipelinesResponse } from '../../generated/CallQueuePipelinesResponse';
import type { PipelinesFetch } from './useQueuePipelines';
import type { QueueFilters } from './queueModel';
import type { CrmMetadataResponse } from '../../generated/CrmMetadataResponse';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { MetadataFetch } from './useResultDefinitions';
import type { UserFetch } from './useCurrentUser';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import type { CatalogFetch } from './usePropertyCatalog';
import { DOCK_STORAGE_KEY } from './dockModel';
import { PROPS_STORAGE_KEY } from './propertyModel';

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
      pipeline: f.pipeline, owner: f.owner === '' ? 'all' : f.owner, role: 'admin', teams: [],
      stages: [...(f.stages.length ? f.stages : eligibleStageIds(f.pipeline))].sort(), due: f.due, sort: f.sort,
      q: f.q.trim() || null, limit: 50,
      next_from: f.nextFrom || null, next_to: f.nextTo || null, last_from: f.lastFrom || null, last_to: f.lastTo || null,
    },
    partial: { missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0 },
    generated_at: '2026-10-05T03:00:00Z',
    ...over,
  };
}

/**
 * bpo_57 その他理由: スナップショット (mocProperties.ts) には無いが、/api/crm/metadata の許可リストには入っている。
 * 型・ラベルは 2026-10-08 に HubSpot の Deal プロパティ定義で確認 (string / その他理由)
 */
const BPO_57 = { name: 'bpo_57', label: 'その他理由', type: 'string', fieldType: 'text', options: [] };

/** GET /api/crm/metadata の応答の形をした架空の定義 (mocProperties.ts のスナップショット + bpo_57) */
export function metadataFromMoc(only?: readonly string[]): CrmMetadataResponse {
  return {
    properties: [...Object.values(MOC_DEAL_PROPERTIES), BPO_57].filter(p => !only || only.includes(p.name)).map(p => ({
      object_type: 'deals', name: p.name, label: p.label, property_type: p.type, field_type: p.fieldType, options: p.options,
    })),
    pipelines: [], fetched_at: '2026-10-08T00:00:00Z', hubspot_ms: 1, total_ms: 1, cache_hit: false,
  };
}

/** 定義の取得を溜めておき、テストが応答する偽の取得関数 */
export function metadataStub() {
  const calls: ((r: ApiResult<CrmMetadataResponse>) => void)[] = [];
  const fetcher: MetadataFetch = () => new Promise(resolve => { calls.push(resolve); });
  return { calls, fetcher };
}
/** すぐ定義を返す取得関数 */
export const okMetadataFetch: MetadataFetch = () => Promise.resolve({ ok: true, data: metadataFromMoc() });

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

/** ログイン中の人 (テストでは固定の架空のアドレス) */
export const TEST_USER = 'caller-a@example.invalid';
export const userFetchFor = (email: string): UserFetch => () => Promise.resolve({ ok: true, data: { user_email: email } });
export const okUserFetch: UserFetch = userFetchFor(TEST_USER);

/**
 * GET /api/crm/call-queue/pipelines の応答の形をした架空の名前。bpo_リクロジは架空サンプルと同じ名前、
 * アポ前 (`default`) は「アポ前」「アポ前の未済」など、それ以外は名前なし。`unknown` は表に無いステージ (bpo_リクロジ)
 */
export function pipelinesResponse(unknown: { id: string; label: string }[] = []): CallQueuePipelinesResponse {
  const bpo = FIXTURE_PIPELINES[0];
  return {
    default_pipeline: '753186575', labels_available: true,
    pipelines: LIVE_PIPELINES.map(p => ({
      id: p.id,
      label: p.id === bpo?.id ? 'bpo_リクロジ' : p.id === 'default' ? 'アポ前' : null,
      stages: p.stages.map(s => ({
        id: s.id, rule: s.rule,
        label: p.id === bpo?.id ? (bpo.stages.find(x => x.id === s.id)?.label ?? null) : p.id === 'default' ? `アポ前の${s.id}` : null,
      })),
      unknown_stages: p.id === bpo?.id ? unknown.map(u => ({ ...u, rule: 'exclude' })) : [],
    })),
  };
}
export const okPipelinesFetch: PipelinesFetch = () => Promise.resolve({ ok: true, data: pipelinesResponse() });

/** 「プロパティ」パネルの項目の一覧 (架空サンプルと同じ一覧を返す。通信しない) */
export const okCatalogFetch: CatalogFetch = () => Promise.resolve({ ok: true, data: FIXTURE_CATALOG });

/** パネルの配置・表示する項目の選択 (localStorage) を消す (前のテストの配置を持ち越さない) */
export function resetDockStorage(): void {
  try {
    window.localStorage.removeItem(DOCK_STORAGE_KEY);
    window.localStorage.removeItem(PROPS_STORAGE_KEY);
  } catch { /* 無い環境 */ }
}
