import type { DealRow, DealBoardResponse } from './dealBoardModel';
import { demoOwnerTodo } from './demoOwnerTodo';
export const boardRows: DealRow[] = Array.from({ length: 24 }, (_, i) => ({
  id: String(100 + i), name: `架空の採用支援${String(i + 1).padStart(2, '0')}`, company: '合成事業所',
  pipeline: i === 23 ? '743961896' : '21596025', active: i !== 22, stage: '支援中', consultant: '合成担当A',
  reasons: i === 0 ? [{ kind: 'nps', text: '定期NPSが4以下' }, { kind: 'mtg', text: 'MTGが90日以上途絶', strong: true }] : [],
  first_contract: false, launch: { start: null, elapsed: null, status: 'unavailable', total: null, remaining: null, first: null, first_day: null, weeks: null, live: null },
  months: 3, expiration: '2026-12-31', days_left: 76, last_contact: '2026-10-12', last_mtg: i === 23 ? null : i === 0 ? '2026-07-01' : '2026-10-01',
  recent_applications: i+1, listing_count: i < 2 ? 3 : null, zero_applicants: i === 0, mtg_status: i === 23 ? 'outside' : 'ready', nps: i === 0 ? 4 : 5, applications: 20, interviews: 4, hires: 2, target: 10, achievement: 0.2, cpa: (100000 + i * 10000) / 2, amount: 100000 + i * 10000,
}));
export const boardFixture: DealBoardResponse = { status: 'ready', hubspot_portal_id: '1234567', rows: boardRows, work: demoOwnerTodo, sheet_as_of: '2026-10-15', refreshing: false, refresh_failed: false, applications_status: 'ready' };

export const firstRows: DealRow[] = boardRows.slice(0, 6).map((row, i) => ({ ...row,
  first_contract: i !== 5,
  reasons: [...row.reasons, ...(i === 0 || i === 3 ? [{ kind: 'first_contract' as const, text: `初回契約・開始から ${i === 0 ? '60' : '90'} 日で応募 ${i === 0 ? '9' : '0'} 件` }] : [])],
  launch: {
    start: ['2026-08-17', '2026-08-18', '2026-09-17', '2026-07-18', '2026-09-16', '2026-07-18'][i] ?? null, elapsed: [60, 59, 29, 90, 30, 90][i] ?? 0,
    status: (['below', 'on_track', 'warming', 'below', 'unavailable', 'below'] as const)[i] ?? 'below',
    total: i === 4 ? null : [9, 5, 0, 0, 0, 0][i] ?? 0,
    remaining: i === 0 ? 1 : i === 3 || i === 5 ? 10 : 0,
    first: i === 0 ? '2026-08-24' : i === 1 ? '2026-08-25' : null, first_day: i < 2 ? 8 : null,
    weeks: i === 4 ? null : i === 2 ? [0, 0, 0, 0, 0] : i === 0 ? [0, 2, 1, 0, 1, 0, 0, 5] : i === 1 ? [0, 2, 1, 0, 1, 0, 0, 1] : [0, 0, 0, 0, 0, 0, 0, 0], live: i === 4 ? null : 2,
  },
}));
