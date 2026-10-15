// @vitest-environment happy-dom
import { useState } from 'react';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ListingOverview } from './ListingOverview';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { buildListingEvents } from './listingEventsModel';
import { classifyApplicationReasons } from './reasonCategories';
import { loadApplicationDates } from './listingApplicationDates';

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
const demo = (): JobCopyRecord => { const job = jobs[0]; if (!job) throw new Error('fixture'); return job; };
function Overview({ job = demo() }: { job?: JobCopyRecord }) {
  const [selectedId, onSelect] = useState<string | null>(null);
  return <ListingOverview job={job} events={buildListingEvents(job, [])} billing={[]} market={{ state: 'unset' }} selectedId={selectedId} onSelect={onSelect} onCompare={() => undefined} onOpenDrawer={() => undefined} />;
}
it('同じ応募に属性・応募理由・電話ヒアリング・メモを表示し、カードと往復できる', () => {
  const { container } = render(<Overview />);
  const event = container.querySelector<HTMLElement>(`#application-${'8'.repeat(64)}`);
  if (!event) throw new Error('event');
  fireEvent.click(within(event).getByRole('button'));
  expect(event.textContent).toContain('女性 ・ 38 ・ 大分県大分市');
  expect(event.textContent).toContain('通勤に片道1時間かかるため');
  expect(event.textContent).toContain('勤務条件の優先順位通勤時間、休日、給与の順で希望');
  expect(event.textContent).toContain('【架電】土日休みと、見学できる日程を確認したい。');
  fireEvent.click(within(event).getByRole('button', { name: '電話ヒアリングのこの応募へ' }));
  expect(container.querySelector('details.ph-section')?.hasAttribute('open')).toBe(true);
  const card = container.querySelector<HTMLElement>(`#hearing-${'8'.repeat(64)}`);
  if (!card) throw new Error('card');
  expect(card.textContent).toContain('応募者：38・女性・大分県大分市');
  fireEvent.click(within(event).getByRole('button', { name: /応募.*女性/u }));
  expect(event.classList.contains('lo-open')).toBe(false);
  fireEvent.click(within(card).getByRole('button', { name: 'タイムラインのこの応募へ' }));
  expect(event.classList.contains('lo-open')).toBe(true);
});
it('記録がすべて空の応募だけ、記録がないと表示する', () => {
  const job = demo();
  const row = job.applicationRows?.[1];
  if (!row) throw new Error('row');
  const { container } = render(<Overview job={{ ...job, applicantReasons: undefined }} />);
  const event = container.querySelector<HTMLElement>(`#application-${row.key}`);
  if (!event) throw new Error('event');
  fireEvent.click(within(event).getByRole('button'));
  expect(within(event).getByText('この応募には、応募理由や電話で聞いた内容の記録がありません')).toBeTruthy();
});
it('集計表の電話項目・架電メモを具体値で示し、電話の文も分類する', () => {
  render(<Overview />);
  const table = screen.getByRole('region', { name: '記録欄ごとの件数' });
  for (const name of ['勤務条件の優先順位', '入社希望時期', '架電メモ']) {
    const row = within(table).getByRole('rowheader', { name }).closest('tr');
    expect(row?.textContent).toBe(`${name}1件0件27件`);
  }
  const job = demo();
  const reasons = job.applicantReasons;
  if (!reasons) throw new Error('reasons');
  const classified = classifyApplicationReasons({ ...reasons, selections: [], items: reasons.items.filter(item => item.sourceProperty === 'call_notes') });
  expect(classified?.applications.map(item => [item.key, item.categories])).toEqual([['8'.repeat(64), ['その他']]]);
});
it('共通の応募一覧から属性・理由・電話を一緒に読み込む', async () => {
  const hearing = demo().phoneHearing;
  const reasons = demo().applicantReasons;
  if (!reasons) throw new Error('reasons');
  const rawReasons = { available: true, source: 'hubspot', basis: reasons.basis, source_property: null, fetched_at: reasons.fetchedAt,
    total_applicants: reasons.totalApplicants, total_source_values: reasons.totalSourceValues, source_counts: reasons.sourceCounts,
    missing: reasons.missing, blank: reasons.blank, truncated: false, selections: (reasons.selections ?? []).map(item => ({ applicant: item.applicant, source_property: item.sourceProperty, value: item.value, label: item.label, application_date: item.applicationDate })),
    items: reasons.items.map(item => ({ id: item.id, applicant: item.applicant, text: item.text, source: 'hubspot', source_property: item.sourceProperty, application_date: item.applicationDate, collected_at: null, version_id: null })) };

  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ listing_id: '30', total: 28, missing_date: 0, by_date: { '2026-09-24': 28 }, fetched_at: '2026-10-05T00:00:00Z', applications: demo().applicationRows, phone_hearing: hearing, applicant_reasons: rawReasons }), { headers: { 'Content-Type': 'application/json' } }));
  vi.stubGlobal('fetch', fetch);
  const loaded = await loadApplicationDates('30');
  expect(String(fetch.mock.calls[0]?.[0])).toContain('/listings/30/applicants');
  expect(loaded?.rows[0]?.key).toBe(loaded?.phoneHearing?.items[0]?.applicant);
  expect(loaded?.rows[0]?.age).toBe('38');
  expect(loaded?.reasons?.items.find(item => item.sourceProperty === 'call_notes')?.applicant).toBe(loaded?.phoneHearing?.items[0]?.applicant);
  expect(loaded?.reasons?.sourceCounts.kinmujoukennoyuusenjuni).toEqual({ missing: 27, blank: 0, nonblank: 1 });
});
