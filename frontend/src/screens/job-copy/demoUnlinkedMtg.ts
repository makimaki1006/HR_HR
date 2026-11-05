import type { UnlinkedResponse } from './unlinkedMtgModel';
/** スクリーンショット・確認用。すべて合成データ。 */
export const demoUnlinkedMtg: UnlinkedResponse = {
  status: 'ready', me_email: 'demo@example.test', selected_known: true,
  own_deals: { '11': [{ id: '901', name: '合成商事 採用支援' }, { id: '902', name: '合成工業 採用支援' }, { id: '903', name: '合成運輸 採用支援' }] },
  rows: [
    { uuid: 'demo-1', recording: 'https://example.test/rec/1', date: '2026-11-04', subject: '【合成商事】採用定例MTG', host: '合成担当A', host_owner_id: '11', candidates: [{ id: '901', name: '合成商事 採用支援' }, { id: '904', name: '合成商事 追加プラン' }], other_candidates: 2, selected: null },
    { uuid: 'demo-2', recording: 'https://example.test/rec/2', date: '2026-11-02', subject: '合成工業 振り返り', host: '合成担当A', host_owner_id: '11', candidates: [], other_candidates: 0, selected: null },
    { uuid: 'demo-3', recording: 'https://example.test/rec/3', date: '2026-10-30', subject: '合成運輸 初回打ち合わせ', host: '合成担当A', host_owner_id: '11', candidates: [{ id: '903', name: '合成運輸 採用支援' }], other_candidates: 0, selected: { deal_id: '903', deal_name: '合成運輸 採用支援', at: '2026-11-01 10:30', by: 'demo@example.test' } },
  ],
};
