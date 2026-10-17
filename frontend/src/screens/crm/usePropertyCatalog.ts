import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import type { ApiResult } from '../../api/client';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import { HUBSPOT_CARDS } from './hubspotCards';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import { FIXTURE_APPO_GROUPS } from './fixtureAppoFields';
import { catalogIndex } from './propertyModel';
import type { CatalogEntry, CatalogObject } from './propertyModel';
import type { QueueMode } from './queueModel';
import { metadataErrorMessage } from './useResultDefinitions';

export type CatalogFetch = (signal: AbortSignal) => Promise<ApiResult<CrmPropertyCatalogResponse>>;
export const liveCatalogFetch: CatalogFetch = signal => apiGet<CrmPropertyCatalogResponse>('/api/crm/property-catalog', { signal, timeoutMs: 35_000 });

const p = (name: string, label: string, property_type: string, field_type: string, options: CrmCatalogProperty['options'] = []): CrmCatalogProperty =>
  ({ name, label, property_type, field_type, options, owner_ref: false });
const moc = (name: string): CrmCatalogProperty => {
  const d = MOC_DEAL_PROPERTIES[name];
  return d ? p(d.name, d.label, d.type, d.fieldType, d.options.filter(o => !o.hidden)) : p(name, name, 'string', 'text');
};

/** 架空サンプルで日付として扱うカードの項目 (それ以外は文字) */
const FIXTURE_DATE_PROPS = new Set(['risuto_kadennbi', 'risuto_saikadennbi', 'aposyutokubi', 'bpo_appo_date', 'recruit_media_first_observed_at', 'recruit_media_last_observed_at']);
const FIXTURE_BASE_DEAL_PROPS = new Set(['dealname', 'hubspot_owner_id', 'bpo_13', 'bpo_14', 'bpo_20', 'bpo_10', 'bpo_32', 'bpo_21', 'bpo_22', 'bpo_50', 'bpo_3', 'bpo_4', 'amount']);
/** HubSpot のカード (hubspotCards.ts) の項目を、架空の項目グループとして並べる (表示名はカードと同じ) */
function fixtureCardProps(): CrmCatalogProperty[] {
  const out: CrmCatalogProperty[] = [];
  for (const c of HUBSPOT_CARDS) {
    for (const it of c.items) {
      if (FIXTURE_BASE_DEAL_PROPS.has(it.name) || out.some(x => x.name === it.name)) continue;
      const typed = FIXTURE_TYPED_PROPS[it.name];
      out.push(typed ? { ...typed, label: it.label } : it.name === 'scheduled_business_meeting_date' ? p(it.name, it.label, 'datetime', 'date')
        : FIXTURE_DATE_PROPS.has(it.name) ? p(it.name, it.label, 'date', 'date') : p(it.name, it.label, 'string', 'text'));
    }
  }
  // アポ取得者 (担当者を選ぶ型。新規営業の「TEL_アポ日確定」の必須項目)
  out.push({ ...p('aposyutokusya', 'アポ取得者', 'enumeration', 'select'), owner_ref: true });
  return out;
}

/** 架電結果の入力欄で使う項目の型 (HubSpot の実際の型と同じ。値・選択肢は架空) */
const FIXTURE_TYPED_PROPS: Record<string, CrmCatalogProperty> = {
  bpo_16: moc('bpo_16'),
  bpo_51: p('bpo_51', '接触', 'enumeration', 'select', [{ value: 'true', label: 'はい', hidden: false }]),
  nitteigiri: p('nitteigiri', '日程切り', 'number', 'number'),
  saikadenn_zikan: p('saikadenn_zikan', '再架電時間', 'number', 'number'),
  // 激アツメモは HubSpot では 1 行のテキスト
  risuto_bikou: p('risuto_bikou', '激アツメモ', 'string', 'text'),
};

/** 架空サンプルの項目の一覧 (HubSpot には接続しない。表示名・選択肢は MOC の定義・HubSpot のカードの表示名と架空の値) */
export const FIXTURE_CATALOG: CrmPropertyCatalogResponse = {
  objects: [
    { object_type: 'deals', groups: [
      { name: 'dealinformation', label: '取引情報', properties: [
        p('dealname', '案件名', 'string', 'text'), { ...p('hubspot_owner_id', '案件担当者', 'enumeration', 'select'), owner_ref: true }, p('dealstage', '取引ステージ', 'enumeration', 'select'), p('pipeline', 'パイプライン', 'enumeration', 'select'),
        moc('bpo_13'), moc('bpo_14'), p('bpo_20', '最終架電日', 'date', 'date'), moc('bpo_10'), moc('bpo_32'),
        moc('bpo_21'), moc('bpo_22'), moc('bpo_50'),
      ] },
      { name: 'dealstages', label: '取引ステージのプロパティ', properties: [moc('bpo_3'), moc('bpo_4')] },
      { name: 'call_list', label: '架電リスト(架空のグループ)', properties: fixtureCardProps() },
      { name: 'deal_revenue', label: '取引の売上', properties: [p('amount', '金額', 'number', 'number')] },
      ...FIXTURE_APPO_GROUPS,
    ] },
    { object_type: 'contacts', groups: [
      { name: 'contactinformation', label: 'コンタクト情報', properties: [
        p('lastname', '姓', 'string', 'text'), p('firstname', '名', 'string', 'text'), p('phone', '電話番号', 'string', 'phonenumber'),
        p('mobilephone', '携帯電話番号', 'string', 'phonenumber'), p('jobtitle', '役職', 'string', 'text'), p('email', 'メール', 'string', 'text'),
      ] },
    ] },
    { object_type: 'companies', groups: [
      { name: 'companyinformation', label: '会社情報', properties: [
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
