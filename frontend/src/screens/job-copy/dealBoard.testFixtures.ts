import type { DealRow, DealBoardResponse } from './dealBoardModel';
import { demoOwnerTodo } from './demoOwnerTodo';
export const boardRows: DealRow[] = Array.from({ length: 24 }, (_, i) => ({
  id: String(100 + i), name: `架空の採用支援${String(i + 1).padStart(2, '0')}`, company: '合成事業所',
  pipeline: i === 23 ? '743961896' : '21596025', active: i !== 22, stage: '支援中', consultant: '合成担当A',
  reasons: i === 0 ? [{ kind: 'nps', text: '定期NPSが4以下' }, { kind: 'mtg', text: 'MTGが90日以上途絶', strong: true }] : [],
  months: 3, expiration: '2026-12-31', days_left: 76, last_contact: '2026-10-12', last_mtg: i === 23 ? null : i === 0 ? '2026-07-01' : '2026-10-01',
  mtg_status: i === 23 ? 'outside' : 'ready', nps: i === 0 ? 4 : 5, applications: 20, interviews: 4, hires: 2, target: 10, achievement: 0.2, cpa: (100000 + i * 10000) / 2, amount: 100000 + i * 10000,
}));
export const boardFixture: DealBoardResponse = { status: 'ready', rows: boardRows, work: demoOwnerTodo, sheet_as_of: '2026-10-15', refreshing: false, refresh_failed: false };
