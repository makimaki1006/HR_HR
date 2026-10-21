// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { OwnerFilter, OwnerTodo, ownerParams } from './OwnerTodo';
import { demoOwnerTodo } from './demoOwnerTodo';
beforeEach(() => { window.history.replaceState({ keep: 7 }, '', '/app/job-copy?card=less&owner=11&owner_field=consultant'); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
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
it('7枚の件数カードと、ほかの要対応の件数の1行を表示し、押すと日付をスラッシュで表示した一覧を開く', () => {
  const company = vi.fn(); const deal = vi.fn();
  const push = vi.spyOn(window.history, 'pushState');
  render(<OwnerTodo work={demoOwnerTodo} onOpenCompany={company} onOpenDeal={deal} />);
  expect(document.querySelectorAll('.jh-todo-cards .jh-card')).toHaveLength(7);
  expect([...document.querySelectorAll('.jh-todo-cards .jh-card .jh-card-title')].map(el => el.textContent)).toEqual(['期限切れ', '今日', '7日以内', '契約満了が近い', '接触が空いている', '応募が止まっている', 'MTGが途絶えている']);
  expect(document.querySelector('.jh-rule')).toBeNull();
  expect(document.querySelector('.jh-more')?.textContent).toBe('ほかの要対応：初回で成果が出ていない 1件・定期NPSが低い 0件・採用目標に届いていない 0件・採用単価が高い 0件');
  expect(screen.queryByRole('table')).toBeNull();
  const overdueCard = screen.getByRole('button', { name: /期限切れ/ });
  expect(overdueCard.classList.contains('jh-alert')).toBe(true);
  expect(within(overdueCard).getByText('2')).toBeTruthy();
  fireEvent.click(overdueCard);
  expect(new URLSearchParams(window.location.search).get('card')).toBe('todo_overdue');
  expect(push).toHaveBeenCalledTimes(1);
  const overdue = screen.getByRole('region', { name: '期限切れの一覧' });
  expect(within(overdue).getAllByRole('row')).toHaveLength(3);
  expect(within(overdue).getAllByText('2026/10/15')).toHaveLength(2);
  fireEvent.click(within(overdue).getByRole('button', { name: '合成案件101' }));
  expect(deal).toHaveBeenCalledWith(demoOwnerTodo.actions.overdue[0]);
  const button = within(overdue).getAllByRole('button', { name: '合成事業所' }).at(0);
  if (!button) throw new Error('会社ボタンがありません');
  fireEvent.click(button);
  expect(company).toHaveBeenCalledWith({ id: '901', name: '合成事業所' });
  fireEvent.click(screen.getByRole('button', { name: /契約満了が近い/ }));
  expect(screen.queryByRole('region', { name: '期限切れの一覧' })).toBeNull();
  expect(screen.getByText('満了まで 30 日')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /契約満了が近い/ }));
  expect(screen.queryByRole('table')).toBeNull();
});
it('198件を20件ずつ表示し、末尾の18件と担当変更時のページリセットを確認する', () => {
  const sample = demoOwnerTodo.actions.overdue[0];
  if (!sample) throw new Error('合成データがありません');
  const work = { ...demoOwnerTodo, actions: { ...demoOwnerTodo.actions, overdue: Array.from({ length: 198 }, (_, i) => ({ ...sample, id: String(1000 + i), name: `合成案件${String(1000 + i)}` })) } };
  render(<><OwnerFilter work={work} /><OwnerTodo work={work} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} /></>);
  fireEvent.click(screen.getByRole('button', { name: /期限切れ/ }));
  expect(screen.getByText('1〜20件 ／ 全198件')).toBeTruthy();
  expect(document.querySelectorAll('.jh-todo-table tbody tr')).toHaveLength(20);
  expect(screen.getByRole<HTMLButtonElement>('button', { name: '前の20件' }).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: '次の20件' }));
  expect(screen.getByText('21〜40件 ／ 全198件')).toBeTruthy();
  expect(screen.getByRole('button', { name: '合成案件1020' })).toBeTruthy();
  expect(new URLSearchParams(window.location.search).get('todo_page')).toBe('2');
  for (let i = 2; i < 10; i++) fireEvent.click(screen.getByRole('button', { name: '次の20件' }));
  expect(screen.getByText('181〜198件 ／ 全198件')).toBeTruthy();
  expect(document.querySelectorAll('.jh-todo-table tbody tr')).toHaveLength(18);
  expect(screen.getByRole<HTMLButtonElement>('button', { name: '次の20件' }).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '22' } });
  expect(new URLSearchParams(window.location.search).get('todo_page')).toBeNull();
  expect(screen.getByText('1〜20件 ／ 全198件')).toBeTruthy();
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

it('要対応が1件以上なら期限切れと同じ警告色、0件なら通常色', () => {
  const work = { ...demoOwnerTodo, signals: (demoOwnerTodo.signals ?? []).map(row => ({ ...row, reasons: (row.reasons ?? []).filter(r => r.kind === 'contact') })) };
  render(<OwnerTodo work={work} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('button', { name: /接触が空いている/ }).classList.contains('jh-alert')).toBe(true);
  for (const name of ['契約満了が近い', '応募が止まっている', '初回で成果が出ていない']) expect(screen.getByRole('button', { name: new RegExp(name) }).classList.contains('jh-alert')).toBe(false);
});
