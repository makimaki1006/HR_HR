import columns from '../../generated/jobgen/columns.json';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { HubSpotVersions } from './hubspotListings';
export function draftFixture(): DraftSnapshot {
  return { schema_version: 1, draft_id: '10000000-0000-4000-8000-000000000001', created_at: '2026-10-11T00:00:00Z', source_kind: 'csv', review_status: 'pending', row: { ...Object.fromEntries(columns.map(column => [column, ''])), '案件名': '架空配送スタッフ', '仕事内容': '日用品を配送します。', '給与形態': '月給', '基本給与 最小': '270000', '基本給与 最大': '300000', '求人id': 'hidden-job-123', '店舗id': 'hidden-shop-456', '職種id': 'hidden-role-42' }, facts: { salary: { value: '月給270,000円〜300,000円', evidence_quote: '給与は月給270,000円〜300,000円です。', status: 'verified' }, work_location: { value: '大分県大分市', evidence_quote: '勤務地は大分県大分市です。', status: 'verified' }, holidays: { value: '土日休み', evidence_quote: '土日休みです。', status: 'verified' } } };
}
export function versionsFixture(): HubSpotVersions {
  return { listing: { id: '30', media: 'hrh', media_job_id: 'hidden-hrh-job', account_id: null, title: '架空配送スタッフ', prefecture: '大分県', municipality: '別府市', category: '配送ドライバー', publication_status: '掲載中', last_csv_detected_at: null, application_count: null }, versions: [{ written_at: '2026-10-01T00:00:00Z', body: '案件名：架空配送スタッフ\n仕事内容：日用品を配送します。\n給与形態：月給\n基本給与 最小：250000\n基本給与 最大：280000\n自由項目1のタイトル：休日\n自由項目1の内容：土日休み', image_urls: null }], history_counts: {}, history_may_be_incomplete: false, drafts: [draftFixture()], draft_revision: 'a'.repeat(64), can_write_drafts: true };
}
