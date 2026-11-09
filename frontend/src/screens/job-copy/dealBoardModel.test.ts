import { expect, it } from 'vitest';
import { applicationChange, summarizeApplications, countDealReasons, sortDeals } from './dealBoardModel';
import { boardRows } from './dealBoard.testFixtures';
it('印の本数、金額の順で並べ、欠けた値は向きによらず末尾', () => {
  expect(sortDeals(boardRows, 'priority', true).slice(0, 3).map(r => r.id)).toEqual(['100', '123', '122']);
  const [first, second, third] = boardRows;
  if (!first || !second || !third) throw new Error('並べ替え用の3件がありません');
  const rows = [{ ...first, amount: null }, second, third];
  expect(sortDeals(rows, 'amount', true).map(r => r.amount)).toEqual([120000, 110000, null]);
  expect(sortDeals(rows, 'amount', false).map(r => r.amount)).toEqual([110000, 120000, null]);
});

it('印ごとの件数は同じ案件の同じ印を重ねず、印のない案件も一覧に残す', () => {
  const [first, second] = boardRows;
  if (!first || !second) throw new Error('集計用の2件がありません');
  expect(countDealReasons([first, second, { ...first, id: '200', reasons: [...first.reasons, ...first.reasons] }])).toEqual({ nps: 2, mtg: 2 });
  expect(countDealReasons([])).toEqual({});
});

it('初回は目安より少ない案件を先にし、日数の長い順に並べる', async () => {
  const { firstRows } = await import('./dealBoard.testFixtures');
  const { sortLaunch, launchLabel } = await import('./dealBoardModel');
  expect(sortLaunch(firstRows.filter(r => r.first_contract)).map(r => r.id)).toEqual(['103', '100', '101', '104', '102']);
  expect(firstRows.map(r => launchLabel(r, false))).toEqual(['目安より少ない（あと1件）', '目安どおり', '立ち上がり中', '目安より少ない（あと10件）', '未取得', '目安より少ない（あと10件）']);
  const missing = firstRows[4];
  if (!missing) throw new Error('未取得の案件がありません');
  expect(launchLabel(missing, true)).toBe('集計中');
});

it('直近N週は今日を含む7×N日間で、日付を選ぶときは両方そろい前後が正しいときだけ範囲になる', async () => {
  const { periodRange, jstToday, parseWeeks } = await import('./dealBoardModel');
  expect(periodRange('weeks', 4, null, null, '2026-10-21')).toEqual({ from: '2026-09-24', to: '2026-10-21' });
  expect(periodRange('weeks', 1, null, null, '2026-10-21')).toEqual({ from: '2026-10-15', to: '2026-10-21' });
  expect(periodRange('dates', 4, '2026-10-01', '2026-10-10', '2026-10-21')).toEqual({ from: '2026-10-01', to: '2026-10-10' });
  expect(periodRange('dates', 4, '2026-10-10', '2026-10-01', '2026-10-21')).toBeNull();
  expect(periodRange('dates', 4, '2026-10-01', null, '2026-10-21')).toBeNull();
  expect(periodRange('all', 4, null, null, '2026-10-21')).toBeNull();
  expect(periodRange('contract', 4, null, null, '2026-10-21')).toBeNull();
  expect(jstToday(Date.parse('2026-10-20T16:00:00Z'))).toBe('2026-10-21');
  expect([parseWeeks(null), parseWeeks('0'), parseWeeks('12'), parseWeeks('x')]).toEqual([4, 4, 12, 4]);
});

it('応募ゼロは期間ごとに、求人票が付いた取引だけを数え、数えられない取引は別にする。面接は空を0に混ぜない', async () => {
  const { isZeroApplicant, periodUnknown, hasNoInterview, interviewMissing } = await import('./dealBoardModel');
  const base = boardRows[5];
  if (!base) throw new Error('fixture missing');
  const deal = { ...base, active: true, listing_count: 2, first_contract: true, zero_applicants: true, contract_applications: 0, period_applications: 0 };
  expect(isZeroApplicant(deal, 'all')).toBe(true);
  expect(isZeroApplicant(deal, 'weeks')).toBe(true);
  expect(isZeroApplicant(deal, 'contract')).toBe(true);
  expect(isZeroApplicant({ ...deal, first_contract: false }, 'contract')).toBe(false);
  expect(isZeroApplicant({ ...deal, period_applications: 1 }, 'dates')).toBe(false);
  expect(isZeroApplicant({ ...deal, listing_count: 0 }, 'weeks')).toBe(false);
  expect(isZeroApplicant({ ...deal, listing_count: null }, 'weeks')).toBe(false);
  expect(isZeroApplicant({ ...deal, active: false }, 'weeks')).toBe(false);
  expect(isZeroApplicant({ ...deal, period_applications: null }, 'weeks')).toBe(false);
  expect(periodUnknown({ ...deal, period_applications: null }, 'weeks')).toBe(true);
  expect(periodUnknown({ ...deal, listing_count: 0, period_applications: null }, 'weeks')).toBe(false);
  expect(periodUnknown({ ...deal, contract_applications: null }, 'contract')).toBe(true);
  expect(periodUnknown({ ...deal, period_applications: null }, 'all')).toBe(false);
  expect([hasNoInterview({ ...deal, interviews: 0 }), hasNoInterview({ ...deal, interviews: null }), hasNoInterview({ ...deal, interviews: 2 })]).toEqual([true, false, false]);
  expect([interviewMissing({ ...deal, interviews: 0 }), interviewMissing({ ...deal, interviews: null })]).toEqual([false, true]);
});

it('まとめは応募IDで重複除去し、日付不明と未取得を区別する', () => {
  const base = boardRows[0];
  if (!base) throw new Error('fixture missing');
  const a = { ...base, recent_application_ids: ['a', 'b'], previous_application_ids: ['p'], unknown_application_ids: ['u'] };
  const b = { ...base, id: 'other', recent_application_ids: ['b', 'c'], previous_application_ids: ['p', 'q'], unknown_application_ids: ['u', 'v'] };
  expect(summarizeApplications([a, b])).toEqual({ recent: 3, previous: 2, change: 1, unknown: 2, missing: 0, outOfScope: 0 });
  expect(summarizeApplications([a])).toEqual({ recent: 2, previous: 1, change: 1, unknown: 1, missing: 0, outOfScope: 0 });
  expect(summarizeApplications([{ ...base, recent_application_ids: null }]).missing).toBe(1);
  expect(summarizeApplications([a, { ...base, applications_out_of_scope: true, recent_application_ids: null }])).toEqual({ recent: 2, previous: 1, change: 1, unknown: 1, missing: 0, outOfScope: 1 });
  expect(summarizeApplications([])).toEqual({ recent: 0, previous: 0, change: 0, unknown: 0, missing: 0, outOfScope: 0 });
  expect([applicationChange(3), applicationChange(0), applicationChange(-2), applicationChange(null)]).toEqual(['+3件', '0件', '-2件', '未取得']);
});
