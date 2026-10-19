// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ConsultantRisk, jobsHref, riskGroups } from './ConsultantRisk';
import { ConsultantMenu } from './ConsultantMenu';
import { detailFixture, riskFixture, riskRow, scopeFixture } from './consultantRisk.testFixtures';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
function mock(single = true, first = false) {
  const calls: string[] = [];
  vi.stubGlobal('fetch', vi.fn((input: string) => {
    calls.push(input); const path = new URL(input, window.location.origin);
    const data = path.pathname.endsWith('consultant-scope') ? { ...scopeFixture, owner: path.searchParams.get('owner') ?? '11', owner_field: path.searchParams.get('owner_field') ?? 'consultant' } : path.pathname.endsWith('consultant-risk') ? { ...riskFixture, comparison: first ? 'first' : 'ready', rows: path.searchParams.get('owner') === '22' ? [] : [riskRow] } : path.pathname.includes('deal-detail') ? detailFixture : path.pathname.endsWith('/listings') ? { status: 'ready', total: single ? 1 : 2, next_offset: null, listings: [{ id: '501' }] } : { hubspot_portal_id: '123' };
    return Promise.resolve(new Response(JSON.stringify(data), { headers: { 'content-type': 'application/json' } }));
  })); return calls;
}
it('名札単位で新旧を分け、初日・DBなしは一覧にする', () => {
  const groups = riskGroups(riskFixture);
  expect(groups.map(g => [g.title, g.rows.length])).toEqual([['今日わかった', 1], ['前から出ていて直っていない', 1]]);
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
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11'); mock(single);
  render(<ConsultantRisk />);
  const fresh = await screen.findByRole('region', { name: '今日わかった' });
  expect(fresh.textContent).toContain('NPSが4以下'); expect(fresh.textContent).not.toContain('接触の記録が無い');
  const ongoing = screen.getByRole('region', { name: '前から出ていて直っていない' });
  expect(ongoing.textContent).toContain('2026/10/16から');
  fireEvent.click(within(fresh).getByRole('button', { name: riskRow.name }));
  const dialog = await screen.findByRole('dialog');
  expect(await within(dialog).findByText('採用状況を確認し、次の打ち合わせを決めた。')).toBeTruthy();
  expect(within(dialog).getByRole('button', { name: '終わった → 次の予定' })).toBeTruthy();
  const link = await within(dialog).findByRole('link', { name: '求人を見る' });
  expect(link.getAttribute('href')).toContain('deal=101'); expect(link.getAttribute('href')?.includes('listing=501')).toBe(single);
  expect((await within(dialog).findByRole('link', { name: 'HubSpotで開く' })).getAttribute('href')).toBe('https://app.hubspot.com/contacts/123/record/0-3/101');
});
it('保存初日は翌日の比較案内を表示する', async () => {
  window.history.replaceState(null, '', '/app/job-copy?view=risk&owner=11'); mock(true, true);
  render(<ConsultantRisk />);
  expect(await screen.findByText('前回と比べられるのは明日のデータから')).toBeTruthy();
  expect(screen.queryByRole('region', { name: '今日わかった' })).toBeNull();
});
