// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { OwnerFilter, OwnerTodo, ownerParams } from './OwnerTodo';
import { demoOwnerTodo } from './demoOwnerTodo';
beforeEach(() => { window.history.replaceState({ keep: 7 }, '', '/app/job-copy?card=less&owner=11&owner_field=consultant'); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });
it('担当の種類・ほかの人・全員・自分をreplaceで切り替え、ほかのクエリを保つ', () => {
  const replace = vi.spyOn(window.history, 'replaceState'); const push = vi.spyOn(window.history, 'pushState');
  render(<OwnerFilter work={demoOwnerTodo} />);
  expect(screen.getByLabelText<HTMLSelectElement>('担当の種類').value).toBe('consultant');
  expect([...screen.getByLabelText<HTMLSelectElement>('担当の種類').options].map(o => o.textContent)).toEqual(['コンサル担当', '案件の担当者']);
  // 担当者は文字で絞れる欄。候補は在籍中→退職者の順で、表示名で選ぶ。
  expect(screen.getByRole<HTMLInputElement>('combobox', { name: '担当者' }).value).toBe('合成担当A');
  expect([...document.querySelectorAll('datalist option')].map(o => o.getAttribute('value'))).toEqual(['合成担当A', '合成担当B（退職）']);
  fireEvent.change(screen.getByLabelText('担当の種類'), { target: { value: 'hubspot_owner_id' } });
  expect(new URLSearchParams(window.location.search).get('owner_field')).toBe('hubspot_owner_id');
  // 途中まで打っただけでは切り替えない。
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '合成担当' } });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('11');
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '合成担当B（退職）' } });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('22');
  fireEvent.click(screen.getByRole('button', { name: '全員' }));
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('all');
  fireEvent.click(screen.getByRole('button', { name: '自分' }));
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('me');
  expect(new URLSearchParams(window.location.search).get('card')).toBe('less');
  expect(window.history.state).toEqual({ keep: 7 }); expect(replace).toHaveBeenCalledTimes(4); expect(push).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});
it('7枚の件数カードと、ほかのラベルの札・案件一覧へ移る行を表示し、押すと日付をスラッシュで表示した一覧を開く', () => {
  const company = vi.fn(); const deal = vi.fn();
  const push = vi.spyOn(window.history, 'pushState');
  render(<OwnerTodo work={demoOwnerTodo} onOpenCompany={company} onOpenDeal={deal} />);
  expect(document.querySelectorAll('.jh-todo-cards .jh-card')).toHaveLength(7);
  expect([...document.querySelectorAll('.jh-todo-cards .jh-card .jh-card-title')].map(el => el.textContent)).toEqual(['期限切れ', '今日', '7日以内', '契約満了が近い', '接触が空いている', '応募が止まっている', 'MTGが途絶えている']);
  expect(document.querySelector('.jh-rule')).toBeNull();
  // ほかのラベルは0件も薄い札で残す。初回で成果が出ていないは札にせず、案件一覧へ移る青い下線。
  const others = screen.getByRole('group', { name: 'ほかのラベル' });
  expect(within(others).getAllByRole('button').map(b => [b.textContent, b.classList.contains('is-zero')])).toEqual([['満了後も稼働中 0件', true], ['定期NPSが低い 0件', true], ['採用目標に届いていない 0件', true], ['採用単価が高い 0件', true]]);
  expect(document.querySelector('.jh-go-line')?.textContent).toBe('初回で成果が出ていない 1件→ 案件一覧で見る');
  expect(screen.getByRole('button', { name: 'それ以降の予定 1件' }).getAttribute('aria-pressed')).toBe('false');
  expect(screen.queryByRole('table')).toBeNull();
  const overdueCard = screen.getByRole('button', { name: /期限切れ/ });
  expect(overdueCard.classList.contains('jh-alert')).toBe(true);
  expect(within(overdueCard).getByText('2')).toBeTruthy();
  expect(overdueCard.querySelector('.jh-card-cue')?.textContent).toBe('一覧を見る ▾');
  fireEvent.click(overdueCard);
  expect(overdueCard.querySelector('.jh-card-cue')?.textContent).toBe('閉じる ▴');
  // 一覧は押した段（予定）のすぐ下に開き、ラベルの段より前にある。
  const planRegion = screen.getByRole('region', { name: '期限切れの一覧' });
  expect(planRegion.compareDocumentPosition(screen.getByRole('heading', { name: 'ラベルが付いた案件' })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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
  // ラベルの段の一覧は、その段のカードの下に開く。
  expect(screen.getByRole('heading', { name: 'ラベルが付いた案件' }).compareDocumentPosition(screen.getByRole('region', { name: '契約満了が近いの一覧' })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '契約満了が近いの一覧を閉じる' }));
  expect(screen.queryByRole('table')).toBeNull();
  expect(new URLSearchParams(window.location.search).get('card')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: '案件一覧で見る' }));
  expect(new URLSearchParams(window.location.search).get('view')).toBe('deals');
  expect(new URLSearchParams(window.location.search).get('deals_first')).toBe('yes');
});
it('満了後も稼働中は件数の1行に出し、押すと一覧にHubSpotで開くリンクを出す。MTGの基準は60日以上と説明する', () => {
  const base = demoOwnerTodo.signals?.[0];
  if (!base) throw new Error('合成データがありません');
  const work = { ...demoOwnerTodo, hubspot_portal_id: '123', signals: [{ ...base, id: '777', name: '合成案件777', reasons: [{ kind: 'expired' as const, text: '満了日から 14 日が過ぎている' }] }] };
  render(<OwnerTodo work={work} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('button', { name: '満了後も稼働中 1件' }).classList.contains('is-zero')).toBe(false);
  expect([...document.querySelectorAll('.jh-todo-cards .jh-card .jh-card-title')]).toHaveLength(7);
  fireEvent.click(screen.getByRole('button', { name: '満了後も稼働中 1件' }));
  const list = screen.getByRole('region', { name: '満了後も稼働中の一覧' });
  expect(within(list).getByText('満了日から 14 日が過ぎている')).toBeTruthy();
  const out = within(list).getByRole('link', { name: 'HubSpotで開く' });
  expect(out.getAttribute('href')).toBe('https://app.hubspot.com/contacts/123/record/0-3/777');
  expect(out.textContent).toBe('↗ HubSpot');
  expect(out.getAttribute('target')).toBe('_blank');
  expect(screen.getByText('MTGが途絶えている：最後のMTGから60日以上')).toBeTruthy();
  expect(screen.getByText('満了後も稼働中：満了日が今日より前で、終了していないステージ（ステージはHubSpotで直す）')).toBeTruthy();
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
  fireEvent.change(screen.getByLabelText('担当者'), { target: { value: '合成担当B（退職）' } });
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

it('ラベルが1件以上なら橙（注意）、0件なら灰。期限切れは件数があるときだけ赤', () => {
  const work = { ...demoOwnerTodo, signals: (demoOwnerTodo.signals ?? []).map(row => ({ ...row, reasons: (row.reasons ?? []).filter(r => r.kind === 'contact') })) };
  const { unmount } = render(<OwnerTodo work={work} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('button', { name: /接触が空いている/ }).className).toBe('jh-card jh-warn');
  for (const name of ['契約満了が近い', '応募が止まっている']) expect(screen.getByRole('button', { name: new RegExp(name) }).className).toBe('jh-card jh-zero');
  expect(screen.getByRole('button', { name: /期限切れ/ }).className).toBe('jh-card jh-alert');
  unmount();
  render(<OwnerTodo work={{ ...work, actions: { ...work.actions, overdue: [] } }} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(screen.getByRole('button', { name: /期限切れ/ }).className).toBe('jh-card jh-zero');
});
it('カードを押すと、開いた一覧の見出しまで画面が動く', () => {
  const scroll = vi.fn();
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, writable: true, value: scroll });
  const raf = vi.spyOn(window, 'requestAnimationFrame').mockImplementation(callback => { callback(0); return 0; });
  window.history.replaceState(null, '', '/app/job-copy?owner=11');
  render(<OwnerTodo work={demoOwnerTodo} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(scroll).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: /期限切れ/ }));
  expect(scroll).toHaveBeenCalledTimes(1);
  expect(scroll.mock.contexts[0]).toBe(screen.getByRole('region', { name: '期限切れの一覧' }));
  expect(scroll).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
  fireEvent.click(screen.getByRole('button', { name: /契約満了が近い/ }));
  expect(scroll).toHaveBeenCalledTimes(2);
  raf.mockRestore();
});
it('URL に card= があって最初から開いている一覧にも、表示した直後に画面が動く', () => {
  const scroll = vi.fn();
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, writable: true, value: scroll });
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(callback => { callback(0); return 0; });
  window.history.replaceState(null, '', '/app/job-copy?owner=11&card=todo_overdue');
  render(<OwnerTodo work={demoOwnerTodo} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(scroll).toHaveBeenCalledTimes(1);
  expect(scroll.mock.contexts[0]).toBe(screen.getByRole('region', { name: '期限切れの一覧' }));
});

it('ほかのラベルがすべて0件でも、札を薄く残して並びを変えない', () => {
  render(<OwnerTodo work={{ ...demoOwnerTodo, signals: [] }} onOpenCompany={vi.fn()} onOpenDeal={vi.fn()} />);
  expect(within(screen.getByRole('group', { name: 'ほかのラベル' })).getAllByRole('button').map(b => b.textContent)).toEqual(['満了後も稼働中 0件', '定期NPSが低い 0件', '採用目標に届いていない 0件', '採用単価が高い 0件']);
  expect(document.querySelectorAll('.jh-more .jc-chip.is-zero')).toHaveLength(4);
  // 0件のカードには「一覧を見る ▾」を出さない。
  expect(screen.getByRole('button', { name: /契約満了が近い/ }).textContent).toBe('0件契約満了が近い');
});
