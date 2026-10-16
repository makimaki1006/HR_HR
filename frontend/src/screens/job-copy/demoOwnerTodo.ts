import type { WorkReady, TodoRow } from './OwnerTodo';
const row = (id: string, due: string, content: string): TodoRow => ({ id, name: `合成案件${id}`, company_id: '901', company: '合成事業所', stage: '対応中', content, due, consultant: '合成担当A' });
/** スクリーンショット・E2E専用。すべて合成データ。 */
export const demoOwnerTodo: WorkReady = {
  status: 'ready', owner: '11', owner_field: 'consultant', notice: null, refresh_failed: false,
  owners: [{ id: '11', name: '合成担当A', archived: false }, { id: '22', name: '合成担当B（退職）', archived: true }],
  actions: { overdue: [row('101', '2026-10-15', '電話する'), row('102', '2026-10-15', '応募状況を確認する')], today: [row('103', '2026-10-16', '掲載内容を確認する')], week: [row('104', '2026-10-23', '振り返りの連絡をする')], later: [row('105', '2026-10-24', '次回の打ち合わせ')] },
  expirations: [row('106', '2026-11-15', '契約満了')],
};
