import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { fixtureItem } from './queueFixture';
import { formatPhoneForDisplay } from './phone';
import type { SelectedProps } from './propertyModel';

type Values = Record<string, string | null>;
const pickValues = (all: Values, names: readonly string[] | undefined): Values =>
  Object.fromEntries((names ?? []).map(n => [n, all[n] ?? null]));

/**
 * 架空の詳細 (HubSpot には接続しない)。電話番号は 0300000000 台、会社名・氏名・本文は架空。
 * 架空サンプルモードでは発信を無効にするので、番号が実在の番号と重なっても発信されない。
 */
export function fixtureDetail(dealId: string, props?: SelectedProps): WorkspaceResponse | null {
  const item = fixtureItem(dealId);
  if (item === null) return null;
  const phone = item.phone;
  const jobSearch = phone ? `https://www.google.com/search?q=${encodeURIComponent(formatPhoneForDisplay(phone) ?? phone)}+%E6%B1%82%E4%BA%BA&sca_esv=sample` : null;
  // 「プロパティ」パネル用の架空の値 (選んだ項目だけを返す。実データの selected と同じ形)
  const dealValues: Values = {
    dealname: item.deal_name, hubspot_owner_id: item.owner_id, bpo_13: item.next_call_date, bpo_14: item.next_call_time,
    bpo_20: item.last_call_date, bpo_10: item.stop.unreachable_check, bpo_3: null, bpo_4: null, bpo_32: jobSearch, amount: '120000',
    bpo_50: '受付の方は親切。採用担当は午後在席(架空)',
  };
  const [last, first] = (item.contact?.name ?? '').split(/\s+/u);
  const contactValues: Values = item.contact ? {
    lastname: last ?? null, firstname: first ?? null, phone: item.contact.phone, mobilephone: '090-0000-0099', jobtitle: item.contact.job_title,
    email: 'sample@example.invalid',
  } : {};
  const companyValues: Values = item.company ? { name: item.company.name, website: 'https://www.example.com/', industry: '架空業', numberofemployees: '25' } : {};
  return {
    deal: {
      id: item.deal_id, name: item.deal_name, stage_id: item.stage_id, stage_label: item.stage_label, pipeline_id: '753186575',
      owner_id: item.owner_id, amount: '120000', close_date: null, next_call_date: item.next_call_date,
      next_call_time: item.next_call_time, last_call_date: item.last_call_date, stop: item.stop, bpo_phone: null,
      // 求人検索は HubSpot の「URL_求人検索」と同じ形 (余計な項目付き)。ホームページは IANA の例示用ドメイン
      job_search_url: jobSearch,
      homepage_url: 'https://www.example.com/', media_job_urls: null, job_posting_url: null,
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
      industry: '架空業', domain: 'example.invalid', website: null, labels: ['主'], is_primary: true,
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
    partial: [],
    selected: {
      deal: pickValues(dealValues, props?.deals),
      contact: item.contact ? pickValues(contactValues, props?.contacts) : {},
      company: item.company ? pickValues(companyValues, props?.companies) : {},
    },
    hubspot_portal_id: '0', data_scope: '架空データ。HubSpot には接続していません', generated_at: '2026-10-05T03:00:00Z',
    // 架空データは読むたびに作るので、読んだ時刻は今
    fetched_at: new Date().toISOString(), cached: false,
  };
}
