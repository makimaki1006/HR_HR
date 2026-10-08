import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import { catalogIndex } from './propertyModel';
import type { CatalogEntry, CatalogObject } from './propertyModel';
import type { QueueMode } from './queueModel';
import { metadataErrorMessage } from './useResultDefinitions';

export type CatalogFetch = (signal: AbortSignal) => Promise<ApiResult<CrmPropertyCatalogResponse>>;
export const liveCatalogFetch: CatalogFetch = signal => apiGet<CrmPropertyCatalogResponse>('/api/crm/property-catalog', { signal, timeoutMs: 35_000 });

const p = (name: string, label: string, property_type: string, field_type: string, options: CrmCatalogProperty['options'] = []): CrmCatalogProperty =>
  ({ name, label, property_type, field_type, options });
const moc = (name: string): CrmCatalogProperty => {
  const d = MOC_DEAL_PROPERTIES[name];
  return d ? p(d.name, d.label, d.type, d.fieldType, d.options.filter(o => !o.hidden)) : p(name, name, 'string', 'text');
};

/** 架空サンプルの項目の一覧 (HubSpot には接続しない。表示名・選択肢は MOC の定義と架空の値) */
export const FIXTURE_CATALOG: CrmPropertyCatalogResponse = {
  objects: [
    { object_type: 'deals', groups: [
      { name: 'dealinformation', label: 'Deal information', properties: [
        p('dealname', '案件名', 'string', 'text'), p('hubspot_owner_id', '案件担当者', 'enumeration', 'select'),
        moc('bpo_13'), moc('bpo_14'), p('bpo_20', '最終架電日', 'date', 'date'), moc('bpo_10'), moc('bpo_32'),
        moc('bpo_21'), moc('bpo_22'), moc('bpo_50'),
      ] },
      { name: 'dealstages', label: 'Deal Stage Properties', properties: [moc('bpo_3'), moc('bpo_4')] },
      { name: 'deal_revenue', label: 'Deal revenue', properties: [p('amount', '金額', 'number', 'number')] },
    ] },
    { object_type: 'contacts', groups: [
      { name: 'contactinformation', label: 'Contact information', properties: [
        p('lastname', '姓', 'string', 'text'), p('firstname', '名', 'string', 'text'), p('phone', '電話番号', 'string', 'phonenumber'),
        p('mobilephone', '携帯電話番号', 'string', 'phonenumber'), p('jobtitle', '役職', 'string', 'text'), p('email', 'メール', 'string', 'text'),
      ] },
    ] },
    { object_type: 'companies', groups: [
      { name: 'companyinformation', label: 'Company information', properties: [
        p('name', '会社名', 'string', 'text'), p('website', 'ウェブサイトURL', 'string', 'text'), p('industry', '業種', 'string', 'text'),
        p('numberofemployees', '従業員数', 'number', 'number'),
      ] },
    ] },
  ],
  max_selected_per_object: 100, fetched_at: '2026-10-05T03:00:00Z', cache_hit: false,
};

export type CatalogState =
  | { phase: 'loading' }
  | { phase: 'ready'; catalog: CrmPropertyCatalogResponse; index: Record<CatalogObject, Map<string, CatalogEntry>> }
  | { phase: 'error'; message: string };

/**
 * 「プロパティ」パネルの項目の一覧。架空サンプルでは固定の一覧、実データでは GET /api/crm/property-catalog。
 * `enabled` が false の間は取得しない (案件を選ぶまで HubSpot を呼ばない)。失敗は再試行できる
 */
export function usePropertyCatalog(mode: QueueMode, enabled: boolean, fetcher: CatalogFetch = liveCatalogFetch): { state: CatalogState; reload: () => void } {
  const [live, setLive] = useState<CatalogState>({ phase: 'loading' });
  const [attempt, setAttempt] = useState(0);
  const needLive = mode === 'live' && enabled && live.phase === 'loading';
  useEffect(() => {
    if (!needLive) return;
    const ctl = new AbortController();
    void fetcher(ctl.signal).then(r => {
      if (ctl.signal.aborted) return;
      if (!r.ok) { setLive({ phase: 'error', message: metadataErrorMessage(r.error).replace('選択肢', '項目の一覧') }); return; }
      setLive({ phase: 'ready', catalog: r.data, index: catalogIndex(r.data) });
    });
    return () => { ctl.abort(); };
  }, [needLive, fetcher, attempt]);
  const reload = useCallback(() => { setLive({ phase: 'loading' }); setAttempt(a => a + 1); }, []);
  const fixture = useMemo<CatalogState>(() => ({ phase: 'ready', catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) }), []);
  return { state: mode === 'fixture' ? fixture : live, reload };
}
