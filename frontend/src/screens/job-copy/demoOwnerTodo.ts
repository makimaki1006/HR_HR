import type { WorkReady, TodoRow } from './OwnerTodo';
const row = (id: string, due: string, content: string): TodoRow => ({ id, name: `合成案件${id}`, company_id: '901', company: '合成事業所', stage: '対応中', content, due, consultant: '合成担当A', actionyotei: due, actionnaiyou: 'call' });
/** スクリーンショット・E2E専用。すべて合成データ。 */
export const demoOwnerTodo: WorkReady = {
  status: 'ready', owner: '11', owner_field: 'consultant', notice: null, refresh_failed: false,
  owners: [{ id: '11', name: '合成担当A', archived: false }, { id: '22', name: '合成担当B（退職）', archived: true }],
  actions: { overdue: [row('101', '2026-10-15', '電話する'), row('102', '2026-10-15', '応募状況を確認する')], today: [row('103', '2026-10-16', '掲載内容を確認する')], week: [row('104', '2026-10-23', '振り返りの連絡をする')], later: [row('105', '2026-10-24', '次回の打ち合わせ')] },
  expirations: [row('106', '2026-11-15', '契約満了')],
  signals: [{ ...row('106', '2026-11-15', '電話する'), reasons: [{ kind: 'expiration', text: '満了まで 30 日' }, { kind: 'contact', text: '最後の接触から 35 日' }, { kind: 'stalled', text: '直近14日の応募 0 件（掲載中 3 件）' }, { kind: 'first_contract', text: '初回契約・開始から 60 日で応募 4 件' }] }],
};

/** 本番相当の件数で、初期表示の高さとページ分けを確かめる合成データ。すべて本人の担当。 */
export const demoOwnerTodoVolume: WorkReady = {
  ...demoOwnerTodo,
  actions: {
    overdue: Array.from({ length: 198 }, (_, i) => row(String(1000 + i), '2026-10-15', '応募状況を確認する')),
    today: Array.from({ length: 95 }, (_, i) => row(String(2000 + i), '2026-10-16', '掲載内容を確認する')),
    week: Array.from({ length: 112 }, (_, i) => row(String(3000 + i), '2026-10-23', '振り返りの連絡をする')),
    later: Array.from({ length: 154 }, (_, i) => row(String(4000 + i), '2026-10-24', '次回の打ち合わせ')),
  },
  expirations: Array.from({ length: 122 }, (_, i) => row(String(5000 + i), '2026-11-15', '契約満了')),
};
