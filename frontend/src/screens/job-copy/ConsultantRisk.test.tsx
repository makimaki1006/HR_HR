// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ConsultantRisk, jobsHref, riskGroups } from './ConsultantRisk';
import { ConsultantMenu } from './ConsultantMenu';
import { OwnerFilter } from './OwnerTodo';
import { detailFixture, goodFixture, riskFixture, riskRow, scopeFixture } from './consultantRisk.testFixtures';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
function mock(single = true, first = false) {
  const calls: string[] = [];
  vi.stubGlobal('fetch', vi.fn((input: string) => {
    calls.push(input); const path = new URL(input, window.location.origin);
    const data = path.pathname.endsWith('/edit-schema') ? { writes_enabled: false, editable: [] } : path.pathname.endsWith('consultant-scope') ? { ...scopeFixture, owner: path.searchParams.get('owner') ?? '11', owner_field: path.searchParams.get('owner_field') ?? 'consultant' } : path.pathname.endsWith('consultant-good') ? { ...goodFixture, rows: path.searchParams.get('owner') === '22' ? [] : goodFixture.rows } : path.pathname.endsWith('consultant-risk') ? { ...riskFixture, comparison: first ? 'first' : 'ready', rows: path.searchParams.get('owner') === '22' ? [] : riskFixture.rows } : path.pathname.includes('deal-detail') ? detailFixture : path.pathname.endsWith('/listings') ? { status: 'ready', total: single ? 1 : 2, next_offset: null, listings: [{ id: '501' }] } : { hubspot_portal_id: '123', selected: { deal: {} }, partial: [] };
    return Promise.resolve(new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } }));
  })); return calls;
}
it('案件単位で新規優先に分け、初日・DBなしは一覧にする', () => {
  const groups = riskGroups(riskFixture);
  expect(groups.map(g => [g.title, g.rows.map(r => r.deal_id)])).toEqual([['今日わかった', ['101', '104']], ['前から出ていて直っていない', ['103']]]);
  expect(groups.flatMap(g => g.rows.map(r => r.deal_id)).sort()).toEqual(['101', '103', '104']);
  expect(groups[0]?.rows[0]?.risk_flags).toEqual([
    { key: 'NPSが4以下', label: 'NPSが4以下', new: true, since: '2026-10-19' },
    { key: '満了まで60日以内', label: '満了まで60日以内', new: false, since: '2026-10-16' },
  ]);
  expect(riskGroups({ ...riskFixture, comparison: 'first' })).toHaveLength(1);
  expect(riskGroups({ ...riskFixture, comparison: 'unavailable' })).toHaveLength(1);
  expect(jobsHref('?view=risk&owner=22&owner_field=hubspot_owner_id&risk_deal=101', '101', '501')).toBe('/app/job-copy?owner=22&owner_field=hubspot_owner_id&deal=101&listing=501');
});
it('メニューの往復・再読込相当でも担当をURLに保持する', async () => {
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11&owner_field=consultant'); mock();
  render(<ConsultantMenu><p>画面</p></ConsultantMenu>);
  const owner = await screen.findByRole('combobox', { name: '担当者' });
  fireEvent.change(owner, { target: { value: '22' } });
  fireEvent.change(screen.getByRole('combobox', { name: '担当の項目' }), { target: { value: 'hubspot_owner_id' } });
  fireEvent.click(screen.getByRole('button', { name: '案件一覧' }));
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('22');
  expect(new URLSearchParams(window.location.search).get('view')).toBe('deals');
  expect(new URLSearchParams(window.location.search).get('owner_field')).toBe('hubspot_owner_id');
  fireEvent.click(screen.getByRole('button', { name: '危ない案件' }));
  await waitFor(() => { expect(screen.getByRole<HTMLSelectElement>('combobox', { name: '担当者' }).value).toBe('22'); });
  cleanup(); render(<ConsultantMenu><p>画面</p></ConsultantMenu>);
  await waitFor(() => { expect(screen.getByRole<HTMLSelectElement>('combobox', { name: '担当者' }).value).toBe('22'); });
});
it.each([true, false])('新旧の名札・開始日・詳細・求人リンク（求人1件=%s）', async single => {
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11&risk_all=1'); mock(single);
  render(<ConsultantRisk />);
  const fresh = await screen.findByRole('region', { name: '今日わかった' });
  expect(within(fresh).getByRole('heading', { level: 2 }).textContent).toBe('今日わかった 2件');
  const mixed = within(fresh).getByRole('row', { name: new RegExp(riskRow.name) });
  expect(within(mixed).getAllByRole('listitem').map(el => [el.textContent, el.className])).toEqual([
    ['NPSが4以下今日わかった', 'cr-new'],
    ['満了まで60日以内', ''],
  ]);
  // 名札の日付は行に 1 回だけ（いちばん前から出ている名札の日付）
  expect(mixed.textContent.match(/\d{4}\/\d{2}\/\d{2}から/g)).toEqual(['2026/10/16から']);
  expect(within(mixed).getAllByRole('cell')[3]?.textContent).toBe('1日前');
  const ongoing = screen.getByRole('region', { name: '前から出ていて直っていない' });
  expect(within(ongoing).getByRole('heading', { level: 2 }).textContent).toBe('前から出ていて直っていない 1件');
  expect(within(ongoing).getAllByRole('button').map(el => el.textContent)).toEqual(['架空の採用支援C']);
  expect(ongoing.textContent).toContain('2026/10/16から');
  expect(screen.getAllByRole('button', { name: riskRow.name })).toHaveLength(1);
  fireEvent.click(within(fresh).getByRole('button', { name: riskRow.name }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('採用状況を確認し、次の打ち合わせを決めた。')).toBeTruthy();
  expect(within(dialog).getByText('接触：1日前 ／ MTG：2026/10/01')).toBeTruthy();
  expect(detailFixture.events[0]).toMatchObject({ date: '2026-10-18', duration_sec: 120, contact: true });
  expect(within(dialog).getByRole('button', { name: '終わった → 次の予定' })).toBeTruthy();
  const link = await within(dialog).findByRole('link', { name: '求人を見る' });
  expect(link.getAttribute('href')).toContain('deal=101'); expect(link.getAttribute('href')?.includes('listing=501')).toBe(single);
  expect((await within(dialog).findByRole('link', { name: 'HubSpotで開く' })).getAttribute('href')).toBe('https://app.hubspot.com/contacts/123/record/0-3/101');
});
it('既定は名札2本以上。1本だけの案件は件数を添えて外し、切り替えで出す', async () => {
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11'); mock();
  render(<ConsultantRisk />);
  const fresh = await screen.findByRole('region', { name: '今日わかった' });
  expect(within(fresh).getByRole('heading', { level: 2 }).textContent).toBe('今日わかった 1件');
  expect(screen.getByRole('region', { name: '前から出ていて直っていない' }).textContent).toContain('該当する案件はありません。');
  expect(screen.queryByRole('button', { name: '架空の採用支援C' })).toBeNull();
  expect(screen.getByRole('note').textContent).toContain('名札が2本以上の案件 1件を出しています（コンサルKPIの「今日」と同じ基準）。名札が1本だけの案件 2件は出していません。');
  fireEvent.click(screen.getByRole('checkbox', { name: '名札が1本だけの案件も出す' }));
  expect(new URLSearchParams(window.location.search).get('risk_all')).toBe('1');
  expect((await screen.findByRole('button', { name: '架空の採用支援C' })).textContent).toBe('架空の採用支援C');
  expect(screen.getByRole('note').textContent).toContain('名札が1本だけの案件 2件を含めて 3件を出しています。');
  fireEvent.click(screen.getByRole('checkbox', { name: '名札が1本だけの案件も出す' }));
  expect(new URLSearchParams(window.location.search).has('risk_all')).toBe(false);
});
it('保存初日は翌日の比較案内を表示する', async () => {
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11'); mock(true, true);
  render(<ConsultantRisk />);
  const tip = (await screen.findByText('前回と比べられるのは明日のデータからです。')).closest('details');
  expect(tip?.hasAttribute('open')).toBe(false); expect(tip?.querySelector('summary')?.textContent).toContain('前回との比較');
  expect(screen.queryByRole('region', { name: '今日わかった' })).toBeNull();
});

it('いい案件の一覧は全ての印と危ない名札の有無を表示し、同じ詳細を開く', async () => {
  window.history.replaceState(null, '', '/app/job-copy?view=good&owner=11'); const calls = mock();
  render(<ConsultantMenu><ConsultantRisk good /></ConsultantMenu>);
  const list = await screen.findByRole('region', { name: 'いい案件の一覧' });
  expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('いい案件 3件');
  expect(within(list).queryByRole('heading', { level: 2 })).toBeNull();
  expect(within(list).getByRole('columnheader', { name: '理由' })).toBeTruthy();
  expect(within(list).getByText('MTG：採用を継続し、次の募集職種を増やす方向で相談する')).toBeTruthy();
  const row = within(list).getByRole('row', { name: new RegExp(riskRow.name) });
  expect(within(row).getAllByRole('listitem').map(el => [el.textContent, el.className])).toEqual(goodFixture.rows[0]?.good_flags?.map(f => [f.label, 'cr-good']));
  expect(within(row).getByText('危ない名札もあり（2件）')).toBeTruthy();
  expect(screen.queryByText('今日わかった')).toBeNull();
  expect(screen.queryByText('前回との比較を取得できないため、一覧で表示しています。')).toBeNull();
  fireEvent.click(within(row).getByRole('button'));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('採用状況を確認し、次の打ち合わせを決めた。')).toBeTruthy();
  expect(within(dialog).getByRole('button', { name: '終わった → 次の予定' })).toBeTruthy();
  expect(within(dialog).getByRole('heading', { name: 'CS提案内容メモ' })).toBeTruthy();
  expect(within(dialog).getByText('MTG：採用を継続し、次の募集職種を増やす方向で相談する')).toBeTruthy();
  fireEvent.click(within(dialog).getByRole('button', { name: '閉じる' }));
  fireEvent.change(screen.getByRole('combobox', { name: '担当者' }), { target: { value: '22' } });
  await within(list).findByText('該当する案件はありません。');
  expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('いい案件 0件');
  expect(calls.some(url => url.includes('consultant-good?') && url.includes('owner=22'))).toBe(true);
  expect(new URLSearchParams(window.location.search).get('view')).toBe('good');
});

it.each(['', '?view=deals'])('既存画面の担当情報を共通欄へ移し、別の担当APIを要求しない（%s）', async query => {
  window.history.replaceState(null, '', `/app/job-copy${query}`);
  const calls = mock();
  const { container } = render(<ConsultantMenu><OwnerFilter work={scopeFixture} /><p>既存の画面</p></ConsultantMenu>);
  const owner = await screen.findByRole('combobox', { name: '担当者' });
  expect(owner.closest('.cm-menu')).toBeTruthy();
  expect(container.querySelectorAll('.jc-app')).toHaveLength(1);
  expect(screen.getAllByRole('combobox', { name: '担当者' })).toHaveLength(1);
  fireEvent.change(owner, { target: { value: '22' } });
  expect(new URLSearchParams(window.location.search).get('owner')).toBe('22');
  expect(calls).toEqual([]);
});
it('求人の作業中も担当選択を他の画面と同じ形で出し、求人のURLを保持する', async () => {
  window.history.replaceState(null, '', '/app/job-copy?listing=501&owner=22');
  const calls = mock();
  render(<ConsultantMenu><p>求人の本文</p></ConsultantMenu>);
  expect(screen.queryByRole('button', { name: '担当者で絞る' })).toBeNull();
  const owner = await screen.findByRole('combobox', { name: '担当者' });
  expect((owner as HTMLSelectElement).value).toBe('22');
  expect(screen.getByRole('combobox', { name: '担当の項目' })).toBeTruthy();
  expect(screen.getByRole('button', { name: '自分に戻す' })).toBeTruthy();
  expect(calls).toEqual(['/api/job-copy/consultant-scope?owner=22']);
  expect(new URLSearchParams(window.location.search).get('listing')).toBe('501');
});
it('顧客・求人・危ない案件のどの画面でも担当選択の要素が同じ', async () => {
  for (const url of ['?company=9&owner=22', '?listing=501&owner=22', '?view=risk&owner=22']) {
    window.history.replaceState(null, '', `/app/job-copy${url}`); mock();
    const { unmount } = render(<ConsultantMenu><p>本文</p></ConsultantMenu>);
    await screen.findByRole('combobox', { name: '担当者' });
    expect(screen.getAllByRole('combobox').map(e => e.getAttribute('aria-label') ?? e.closest('label')?.textContent.slice(0, 5))).toHaveLength(2);
    expect(screen.getByRole('region', { name: '担当者で絞る' })).toBeTruthy();
    unmount();
  }
});

it('担当者の応答に一覧が無くても画面は落ちず、読み込めなかった案内を出す', async () => {
  window.history.replaceState(null, '', '/app/job-copy?listing=501');
  vi.stubGlobal('fetch', vi.fn(() => Promise.resolve(new Response(JSON.stringify({ status: 'ready', dates: {} }), { status: 200, headers: { 'content-type': 'application/json' } }))));
  render(<ConsultantMenu><p>求人の本文</p></ConsultantMenu>);
  expect(await screen.findByText('担当者を読み込めませんでした。')).toBeTruthy();
  expect(screen.getByText('求人の本文')).toBeTruthy();
});
it('本番の準備中の応答（{"status":"preparing"}）では、顧客・求人の画面に準備中の 1 行を出し、取り直しは間隔を広げる', async () => {
  vi.useFakeTimers();
  try {
    for (const url of ['?company=9&owner=314945657&owner_field=consultant', '?listing=501&owner=314945657&owner_field=consultant']) {
      window.history.replaceState(null, '', `/app/job-copy${url}`);
      const fetchMock = vi.fn(() => Promise.resolve(new Response(JSON.stringify({ status: 'preparing' }), { status: 200, headers: { 'content-type': 'application/json' } })));
      vi.stubGlobal('fetch', fetchMock);
      const { unmount } = render(<ConsultantMenu><p>本文</p></ConsultantMenu>);
      await act(async () => { await vi.advanceTimersByTimeAsync(10); });
      expect(screen.queryByText('担当者を確認しています…')).toBeNull();
      expect(screen.getByRole('status').textContent).toContain('準備しています');
      expect(screen.queryByRole('combobox')).toBeNull();
      await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
      expect(fetchMock.mock.calls.length).toBeLessThanOrEqual(6);
      unmount();
    }
  } finally { vi.useRealTimers(); }
});
