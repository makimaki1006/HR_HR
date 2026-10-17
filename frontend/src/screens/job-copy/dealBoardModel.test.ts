import { expect, it } from 'vitest';
import { countDealReasons, sortDeals } from './dealBoardModel';
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
