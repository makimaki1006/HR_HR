// @vitest-environment happy-dom
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { DealApplicationReasons } from './DealApplicationReasons';
import fixture from '../../../../scripts/e2e/deal_application_reasons.json';

afterEach(cleanup);
it('上位2つの件数と聞けた件数・期間の応募数を表示する', () => {
  render(<DealApplicationReasons value={fixture} />);
  expect(screen.getByText('休日 2件').textContent).toBe('休日 2件');
  expect(screen.getByText(/給与 2件/u).textContent).toBe(' ／ 給与 2件');
  expect(screen.getByText('応募動機を聞けた 3件／2026/09/14 以降の応募 5件').textContent).toBe('応募動機を聞けた 3件／2026/09/14 以降の応募 5件');
});
it('全選択肢と優先順位①②③を具体値で表示する', () => {
  render(<DealApplicationReasons value={fixture} details />);
  const motives = within(screen.getByRole('table', { name: '応募動機の選択件数' }));
  expect(motives.getAllByRole('row').map(row => row.textContent)).toEqual(['選択肢件数', '休日2件', '給与2件', '通勤0件', '旧選択肢（いまの選択肢にない値）1件']);
  const priorities = within(screen.getByRole('table', { name: '勤務条件の優先順位の選択件数' }));
  expect(priorities.getAllByRole('row').map(row => row.textContent)).toEqual(['選択肢①②③', '休日2件0件0件', '給与1件2件0件', '通勤0件0件1件', '旧選択肢（いまの選択肢にない値）0件0件0件']);
  expect(screen.getByText('①を聞けた応募 3件・②2件・③1件／2026/09/14 以降の応募 5件')).toBeTruthy();
});
it('回答0件と未集計を区別する', () => {
  const { rerender } = render(<DealApplicationReasons value={{ ...fixture, answered: [0, 0, 0, 0], top: [] }} />);
  expect(screen.getByText('応募動機の記録はまだありません', { exact: false }).textContent).toContain('応募動機の記録はまだありません');
  expect(screen.getByText('応募動機を聞けた 0件／2026/09/14 以降の応募 5件')).toBeTruthy();
  rerender(<DealApplicationReasons value={null} />);
  expect(screen.getByText('応募動機：集計中').textContent).toBe('応募動機：集計中');
  expect(screen.queryByText(/0件/u)).toBeNull();
});
it('応募を読まない終了済みの取引は、集計中でなく対象外と出す', () => {
  const { container, rerender } = render(<DealApplicationReasons value={null} outOfScope />);
  expect(container.textContent).toBe('応募動機：対象外');
  rerender(<DealApplicationReasons value={null} outOfScope details />);
  expect(container.textContent).toBe('');
});
