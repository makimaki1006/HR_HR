// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { UnlinkedMtg } from './UnlinkedMtg';
import { addManualLink, dealChoices, hasManualLink, jstStamp, removeManualLink } from './unlinkedMtgModel';
import { demoUnlinkedMtg } from './demoUnlinkedMtg';
afterEach(cleanup);
const now = new Date('2026-11-05T01:30:00Z');
it('追記は録画UUID・JSTの日時・メールをタブ区切りで 1 行足す', () => {
  expect(jstStamp(now)).toBe('2026-11-05 10:30');
  expect(addManualLink(null, 'u1', 'a@example.test', now)).toBe('u1\t2026-11-05 10:30\ta@example.test');
  expect(addManualLink('u0\t2026-11-01 09:00\tb@example.test\n', 'u1', 'a@example.test', now)).toBe('u0\t2026-11-01 09:00\tb@example.test\nu1\t2026-11-05 10:30\ta@example.test');
});
it('同じ録画UUID がすでにあれば足さない', () => {
  const base = 'u1\t2026-11-01 09:00\tb@example.test';
  expect(hasManualLink(base, 'u1')).toBe(true);
  expect(addManualLink(base, 'u1', 'a@example.test', now)).toBeNull();
});
it('取り消しはその録画の行だけを消す', () => {
  const base = 'u0\t2026-11-01 09:00\tb@example.test\nu1\t2026-11-05 10:30\ta@example.test\nu2\t2026-11-06 10:30\tc@example.test';
  expect(removeManualLink(base, 'u1')).toBe('u0\t2026-11-01 09:00\tb@example.test\nu2\t2026-11-06 10:30\tc@example.test');
  expect(removeManualLink('u1\t2026-11-05 10:30\ta@example.test', 'u1')).toBe('');
});
it('選択欄は候補を先頭に、担当者の取引が続き重複しない', () => {
  const r = demoUnlinkedMtg.rows?.[0];
  if (!r) throw new Error('fixture');
  expect(dealChoices(r, demoUnlinkedMtg.own_deals).map(c => [c.id, c.candidate])).toEqual([['901', true], ['904', true], ['902', false], ['903', false]]);
});
it('合成データで開催日・件名・選択済みを表示する', () => {
  window.history.replaceState(null, '', '/app/job-copy?view=unlinked&demo=1');
  render(<UnlinkedMtg />);
  expect(screen.getByText('【合成商事】採用定例MTG')).toBeTruthy();
  expect(screen.getAllByRole('button', { name: '結び付ける' })).toHaveLength(2);
  expect(screen.getByText('選択済み・夜の更新で反映')).toBeTruthy();
  expect(screen.getAllByRole('button', { name: '取り消す' })).toHaveLength(1);
});
it('取引は最初は選ばれておらず、選ぶまで結び付けられない', () => {
  window.history.replaceState(null, '', '/app/job-copy?view=unlinked&demo=1');
  render(<UnlinkedMtg />);
  for (const select of screen.getAllByRole('combobox', { name: '結び付ける取引' })) expect((select as HTMLSelectElement).value).toBe('');
  for (const button of screen.getAllByRole('button', { name: '結び付ける' })) expect((button as HTMLButtonElement).disabled).toBe(true);
});
