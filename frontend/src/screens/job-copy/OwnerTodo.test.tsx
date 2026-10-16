// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { OwnerFilter, OwnerTodo, ownerParams } from './OwnerTodo';
import { demoOwnerTodo } from './demoOwnerTodo';
beforeEach(() => { window.history.replaceState({ keep: 7 }, '', '/app/job-copy?card=less&owner=11&owner_field=consultant'); });
afterEach(cleanup);
it('担当の項目・ほかの人・全員・自分をreplaceで切り替え、ほかのクエリを保つ', () => {
  const replace = vi.spyOn(window.history, 'replaceState'); const push = vi.spyOn(window.history, 'pushState');
  render(<OwnerFilter work={demoOwnerTodo} />);
  expect(screen.getByLabelText<HTMLSelectElement>('担当の項目').value).toBe('consultant');
  expect(screen.getByLabelText<HTMLSelectElement>('担当者').value).toBe('11');
  expect(screen.getByRole('group', { name: '退職者' }).textContent).toBe('合成担当B（退職）');
  fireEvent.change(screen.getByLabelText('担当の項目'), { target: { value: 'hubspot_owner_id' } });
  expect(new URLSearchParams(window.location.search).get('owner_field')).toBe('hubspot_owner_id');
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '22' } });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('22');
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: 'all' } });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('all');
  fireEvent.click(screen.getByRole('button', { name: '自分に戻す' }));
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('me');
  expect(new URLSearchParams(window.location.search).get('card')).toBe('less');
  expect(window.history.state).toEqual({ keep: 7 }); expect(replace).toHaveBeenCalledTimes(4); expect(push).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});
it('CSアクションの具体件数・警告色・日付を表示し、それ以降を畳む。案件と会社を開ける', () => {
  const company = vi.fn(); const deal = vi.fn();
  render(<OwnerTodo work={demoOwnerTodo} onOpenCompany={company} onOpenDeal={deal} />);
  expect(screen.getByRole('heading', { name: 'CSアクション 5件' })).toBeTruthy();
  const overdue = screen.getByRole('region', { name: '期限切れのCSアクション' });
  expect(overdue.classList.contains('jh-todo-overdue')).toBe(true);
  expect(within(overdue).getAllByRole('row')).toHaveLength(3);
  expect(within(overdue).getAllByText('2026-10-15')).toHaveLength(2);
  expect(screen.getByText('それ以降（1件）').closest('details')?.open).toBe(false);
  const expiration = screen.getByRole('region', { name: '契約満了が近い' });
  expect(within(expiration).getByText('2026-11-15')).toBeTruthy();
  fireEvent.click(within(overdue).getByRole('button', { name: '合成案件101' }));
  expect(deal).toHaveBeenCalledWith(demoOwnerTodo.actions.overdue[0]);
  const button = within(overdue).getAllByRole('button', { name: '合成事業所' }).at(0);
  if (!button) throw new Error('会社ボタンがありません');
  fireEvent.click(button);
  expect(company).toHaveBeenCalledWith({ id: '901', name: '合成事業所' });
});
it('本人を解決できないときの注記を出し、開発者用の項目名は画面に出さない', () => {
  render(<OwnerFilter work={{ ...demoOwnerTodo, owner: 'all', notice: 'ログインした方の担当者情報が見つからないため、全員を表示しています。' }} />);
  expect(screen.getByRole('status').textContent).toContain('全員を表示');
  expect(document.body.textContent).not.toMatch(/consultant|hubspot_owner_id|owner ID/);
  expect(ownerParams('?deal=10&owner=22&owner_field=hubspot_owner_id&feature=body')).toBe('owner_field=hubspot_owner_id&owner=22');
});

it('全件を取得中はやることを0件と表示しない', () => {
  render(<OwnerTodo work={{ ...demoOwnerTodo, todos_status: 'preparing' }} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toBe('やることを集計しています…');
  expect(screen.queryByText('0件')).toBeNull();
  expect(screen.queryByRole('heading', { name: /CSアクション/ })).toBeNull();
});
