import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { fixtureItem } from './queueFixture';

/**
 * 架空の詳細 (HubSpot には接続しない)。電話番号は 0300000000 台、会社名・氏名・本文は架空。
 * 架空サンプルモードでは発信を無効にするので、番号が実在の番号と重なっても発信されない。
 */
export function fixtureDetail(dealId: string): WorkspaceResponse | null {
  const item = fixtureItem(dealId);
  if (item === null) return null;
  const phone = item.phone;
  return {
    deal: {
      id: item.deal_id, name: item.deal_name, stage_id: item.stage_id, stage_label: item.stage_label, pipeline_id: '753186575',
      owner_id: item.owner_id, amount: '120000', close_date: null, next_call_date: item.next_call_date,
      next_call_time: item.next_call_time, last_call_date: item.last_call_date, stop: item.stop, bpo_phone: null,
      deep_link: item.deep_links.deal,
    },
    dial: phone ? { number: phone, source: 'contact' } : null,
    contacts: item.contact ? [{
      id: item.contact.id, name: item.contact.name, job_title: item.contact.job_title, phone: item.contact.phone,
      mobile: '090-0000-0099', email: 'sample@example.invalid', labels: ['主'], is_primary: true,
      deep_link: `https://example.invalid/contact/${item.contact.id}`,
    }] : [],
    contacts_total: item.contact ? 1 + item.contact.extra_count : 0,
    companies: item.company ? [{
      id: item.company.id, name: item.company.name, phone: item.company.phone, address: '100-0001 東京都 千代田区 架空1-1',
      industry: '架空業', domain: 'example.invalid', labels: ['主'], is_primary: true,
      deep_link: `https://example.invalid/company/${item.company.id}`,
    }] : [],
    companies_total: item.company ? 1 : 0,
    activities: [
      { id: `${dealId}-a1`, kind: 'call', timestamp: '2026-10-03T01:00:00Z', title: '架電(架空)', body: '受付で不在。来週再架電。', direction: 'OUTBOUND', status: 'COMPLETED', duration_ms: 65_000, owner_id: item.owner_id, source: null, via: 'deal', via_id: dealId },
      { id: `${dealId}-a2`, kind: 'note', timestamp: '2026-10-02T01:00:00Z', title: null, body: '採用担当は火曜・木曜の午後が在席。', direction: null, status: null, duration_ms: null, owner_id: item.owner_id, source: null, via: 'deal', via_id: dealId },
      { id: `${dealId}-a3`, kind: 'email', timestamp: '2026-09-30T01:00:00Z', title: 'ご挨拶(架空)', body: '資料を送付します。', direction: 'EMAIL', status: null, duration_ms: null, owner_id: item.owner_id, source: null, via: 'deal', via_id: dealId },
    ],
    activities_truncated: false,
    activity_scope: '架空サンプルです。実データでは、案件に直接つながる通話・メモ・メール・ミーティングと、担当者に直接つながる通話を表示します。',
    partial: [], hubspot_portal_id: '0', data_scope: '架空データ。HubSpot には接続していません', generated_at: '2026-10-05T03:00:00Z',
  };
}
