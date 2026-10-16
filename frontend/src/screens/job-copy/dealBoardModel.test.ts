import { expect, it } from 'vitest';
import { sortDeals } from './dealBoardModel';
import { boardRows } from './dealBoard.testFixtures';
it('印の本数、金額の順で並べ、欠けた値は向きによらず末尾', () => {
  expect(sortDeals(boardRows, 'priority', true).slice(0, 3).map(r => r.id)).toEqual(['100', '123', '122']);
  const [first, second, third] = boardRows;
  if (!first || !second || !third) throw new Error('並べ替え用の3件がありません');
  const rows = [{ ...first, amount: null }, second, third];
  expect(sortDeals(rows, 'amount', true).map(r => r.amount)).toEqual([120000, 110000, null]);
  expect(sortDeals(rows, 'amount', false).map(r => r.amount)).toEqual([110000, 120000, null]);
});
