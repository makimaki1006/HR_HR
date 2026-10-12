// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useState } from 'react';
import columns from '../../generated/jobgen/columns.json';
import { ListingPopup } from './ListingPopup';
import { listingPopupModel } from './listingPopupModel';
import { listingRecord } from './hubspotListings';
import { rowBody } from './manualEditModel';
import { JobCopyScreen } from './JobCopyScreen';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
const api = vi.hoisted(() => vi.fn());
vi.mock('../../api/client', async original => ({ ...await original<typeof import('../../api/client')>(), apiGet: api }));
beforeEach(() => {
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.setAttribute('open', ''); });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.removeAttribute('open'); });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.resetAllMocks(); document.body.style.overflow = ''; localStorage.clear(); });
const description = '荷物を確認します。\r\n\r\n' + '棚から集めて梱包します。\n'.repeat(70) + '最後まで表示する文章です。';
const listing = { id: '1', media: 'hrh' as const, media_job_id: 'hidden-media-id', account_id: null, title: '架空の倉庫スタッフ', prefecture: '東京都', municipality: '江東区', category: null, publication_status: '公開', last_csv_detected_at: null, application_count: 12 };
const record = (fields: Record<string, string> = {}): JobCopyRecord => listingRecord({ listing, versions: [{ written_at: '2026-10-11T00:00:00Z', body: rowBody({ 求人id: 'hidden-job-id', 店舗id: 'hidden-shop-id', 案件名: listing.title, 仕事内容: description, 給与形態: '月給', '基本給与 最小': '213000', '基本給与 最大': '249000', 公開: '公開', ...fields }), image_urls: [] }], history_counts: {}, history_may_be_incomplete: false });
it('keeps the full exact description, orders sections and formats salary with existing functions', () => {
  const job = record({ 通勤経路: '駅から徒歩5分\n車での通勤も可', 勤務時間: '9:00〜18:00', 応募資格: '資格不要', 自由項目1のタイトル: '休日・休暇', 自由項目1の内容: '土日休み', 自由項目2のタイトル: '福利厚生', 自由項目2の内容: '交通費支給', 仕事情報補足1のタイトル: '初日の流れ', 仕事情報補足1の内容: '安全説明から始めます。', 制作メモ: '文章確認済み' });
  job.currentListingFields = { 勤務地: '東京都江東区' };

  const model = listingPopupModel(job, job.versions[0]);
  expect(model.sections.map(section => section.heading)).toEqual(['仕事内容', '給与', '勤務地・アクセス', '勤務時間', '休日・休暇', '応募資格', '福利厚生', '仕事の案内 1', 'その他']);
  expect(model.sections[0]?.entries[0]?.value).toBe(description);
  expect(model.salary).toBe('月給 213,000円〜249,000円');
  expect(model.sections.find(section => section.heading === '福利厚生')?.entries[0]?.value).toBe('交通費支給');
  render(<ListingPopup job={job} version={job.versions[0]} onClose={vi.fn()} />);
  const dialog = screen.getByRole('dialog');
  expect(dialog.querySelector('.jc-listing-popup-section p')?.textContent).toBe(description);
  expect(dialog.textContent).toContain('2026/10/11 09:00 JST');
  expect(dialog.textContent).toContain('初日の流れ');
  expect(dialog.textContent).toContain('文章確認済み');
  expect(dialog.textContent).not.toMatch(/hidden-job-id|hidden-shop-id|hidden-media-id|基本給与 最小|求人id|店舗id/);
  expect(dialog.getAttribute('aria-modal')).toBe('true');
  expect(document.getElementById(dialog.getAttribute('aria-labelledby') ?? '')?.textContent).toBe(listing.title);
});
it('counts exact missing names and retains every other value without exposing a column name', () => {
  const all = Object.fromEntries(columns.map(key => [key, '確認済み']));
  const job = record({ ...all, 勤務時間: '', 応募資格: '', 仕事情報補足4の内容: '' });
  job.currentListingFields = { 勤務地: '東京都江東区', '休日・休暇': '土日休み', 福利厚生: '交通費支給', unmapped_field: '落としてはいけない案内' };
  const model = listingPopupModel(job, job.versions[0]);
  expect(model.missing).toEqual(['仕事の案内4の本文']);
  expect(model.sections.find(section => section.heading === 'その他')?.entries).toContainEqual({ key: 'unmapped_field', label: 'その他の案内', value: '落としてはいけない案内' });
  render(<ListingPopup job={job} version={job.versions[0]} onClose={vi.fn()} />);
  expect(screen.getByText('記載のない任意項目（1件）')).toBeTruthy();
  expect(screen.getByText('記載のない任意項目（1件）').closest('details')?.open).toBe(false);
  expect(screen.getAllByText('記載なし')).toHaveLength(2);
  expect(screen.getByRole('dialog').textContent).not.toContain('unmapped_field');
});
it('keeps numeric codes as review-needed, and never turns missing salaries or counts into zero', () => {
  const job = record({ 給与形態: '01', 雇用形態: '02', '試用・研修の有無': '1', 公開: '01' });
  const model = listingPopupModel(job, job.versions[0]);
  expect(model.salary).toBe('確認が必要 213,000円〜249,000円');
  expect(model.sections.find(section => section.heading === 'その他')?.entries.find(item => item.key === '雇用形態')?.value).toBe('確認が必要');
  expect(model.publication).toBe('確認が必要');
  const missing = record({ 給与形態: '', '基本給与 最小': '', '基本給与 最大': '' }); missing.listingApplicationCount = null;
  expect(listingPopupModel(missing, missing.versions[0])).toMatchObject({ salary: '記載なし', applications: '未取得' });
  missing.listingApplicationCount = 0;
  expect(listingPopupModel(missing, missing.versions[0]).applications).toBe('0件');
});
it('shows AirWork independent conditions only with its latest body, retaining historical full text', () => {
  const job = listingRecord({ listing: { ...listing, media: 'airwork' }, current_fields: { 案件名: listing.title, 仕事内容: '現在の仕事内容。', 給与形態: '月給', 勤務地: '東京都江東区', 勤務時間: '8:00〜17:00', '休日・休暇': '土日休み', 福利厚生: '社会保険', 雇用形態: '正社員' }, versions: [
    { written_at: '2026-10-01T00:00:00Z', body: '過去の仕事内容。\n最後の文章。', image_urls: null },
    { written_at: '2026-10-11T00:00:00Z', body: '現在の仕事内容。', image_urls: null },
  ], history_counts: { shigotonaiyou: 2 }, history_may_be_incomplete: false });
  const old = listingPopupModel(job, job.versions[0]);
  expect(old.sections[0]).toEqual({ heading: '仕事内容', entries: [{ key: '仕事内容', label: '仕事内容', value: '過去の仕事内容。\n最後の文章。' }] });
  expect(old.sections.slice(1).flatMap(section => section.entries).map(entry => entry.value)).toEqual(Array(6).fill('取得できていません'));
  expect(old).toMatchObject({ location: '取得できていません', publication: '未取得', hasCurrentConditions: false });
  const current = listingPopupModel(job, job.versions[1]);
  expect(current.sections.flatMap(section => section.entries).map(item => item.value)).toContain('社会保険');
  expect(current).toMatchObject({ location: '東京都江東区', salary: '月給 金額は未取得', hasCurrentConditions: true });
  const { rerender } = render(<ListingPopup job={job} version={job.versions[1]} onClose={vi.fn()} />);
  rerender(<ListingPopup job={job} version={job.versions[0]} onClose={vi.fn()} />);
  expect(screen.getByRole('dialog').textContent).toContain('過去の仕事内容。\n最後の文章。');
  expect(screen.getByRole('dialog').textContent).not.toContain('現在の仕事内容。');
  expect(screen.getByRole('dialog').textContent).not.toContain('8:00〜17:00');
});
it.each(['close', 'escape', 'outside'])('closes with %s and restores focus and the previous body overflow', method => {
  function Harness() { const [open, setOpen] = useState(false); const job = record(); return <><button onClick={() => { setOpen(true); }}>開く</button>{open && <ListingPopup job={job} version={job.versions[0]} onClose={() => { setOpen(false); }} />}</>; }
  document.body.style.overflow = 'auto'; render(<Harness />);
  const opener = screen.getByRole('button', { name: '開く' }); opener.focus(); fireEvent.click(opener);
  const dialog = screen.getByRole('dialog'); expect(document.activeElement).toBe(dialog); expect(document.body.style.overflow).toBe('hidden');
  if (method === 'close') fireEvent.click(within(dialog).getByRole('button', { name: '求人の内容を閉じる' }));
  if (method === 'escape') fireEvent(dialog, new Event('cancel', { bubbles: false, cancelable: true }));
  if (method === 'outside') fireEvent.click(dialog, { clientX: -10, clientY: -10 });
  expect(screen.queryByRole('dialog')).toBeNull(); expect(document.activeElement).toBe(opener); expect(document.body.style.overflow).toBe('auto');
});
it('does not close on content or padding clicks', () => {
  const close = vi.fn(); render(<ListingPopup job={record()} onClose={close} />);
  fireEvent.click(screen.getByRole('dialog'), { clientX: 0, clientY: 0 });
  fireEvent.click(screen.getByRole('heading', { name: listing.title }));
  expect(close).not.toHaveBeenCalled();
});
it('opens the selected timeline version from the detail heading, initially the latest', () => {
  api.mockResolvedValue({ ok: false, error: new Error('offline') });
  window.history.replaceState(null, '', '/app/job-copy?demo=1');
  localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true');
  render(<JobCopyScreen />);
  const opener = screen.getByRole('button', { name: '求人の内容を確認する' });
  opener.focus(); fireEvent.click(opener);
  expect(screen.getByRole('dialog').textContent).toContain(jobs[0]?.versions[2]?.body);
  fireEvent.click(screen.getByRole('button', { name: '求人の内容を閉じる' }));
  fireEvent.click(screen.getByRole('tab', { name: '求人内容' }));
  fireEvent.click(screen.getByRole('button', { name: /給与・勤務条件変更.*2026\/09\/15/ }));
  fireEvent.click(opener);
  expect(screen.getByRole('dialog').textContent).toContain(jobs[0]?.versions[1]?.body);
  expect(screen.getByRole('dialog').textContent).toContain('2026/09/15 10:30 JST');
  fireEvent.click(screen.getByRole('button', { name: '求人の内容を閉じる' }));
  const main = opener.closest('.jc-main');
  if (!main) throw new Error('詳細の領域がありません');
  main.scrollTop = 500;
  fireEvent.click(screen.getByRole('button', { name: /倉庫内ピッキングスタッフ.*デモ物流B/ }));
  expect(main.scrollTop).toBe(0);
});

it('retains acquired conditions and separately observed images when no body has been acquired', () => {
  const job = listingRecord({ listing, versions: [], history_counts: {}, history_may_be_incomplete: false,
    current_fields: { 案件名: listing.title, 勤務地: '東京都江東区', 福利厚生: '交通費支給' },
    current_images: { observed_at: '2026-10-11T01:00:00Z', image_urls: ['/api/job-copy/image?sample=1'] },
  });
  const model = listingPopupModel(job);
  expect(model.sections.map(section => section.heading)).toEqual(['仕事内容', '給与', '勤務地・アクセス', '勤務時間', '休日・休暇', '応募資格', '福利厚生']);
  expect(model.sections[0]?.entries[0]?.value).toBe('取得できていません');
  expect(model.missing).toEqual([]);
  expect(model.missing).not.toContain('画像');
  expect(model.observedImages?.observedAt).toBe('2026-10-11T01:00:00Z');
  expect(model).toMatchObject({ publication: '公開', applications: '12件', salary: '取得できていません' });
});

it('excludes acquired summary values from the exact missing names and count', () => {
  const all = Object.fromEntries(columns.map(key => [key, '確認済み']));
  const job = record({ ...all, 案件名: '', 公開: '', 勤務時間: '', 応募資格: '', 仕事情報補足4の内容: '' });
  job.listingPublicationStatus = '非公開';
  job.currentListingFields = { '休日・休暇': '土日休み', 福利厚生: '交通費支給' };
  const model = listingPopupModel(job, job.versions[0]);
  expect(model).toMatchObject({ title: listing.title, location: '東京都江東区', publication: '非公開' });
  expect(model.missing).toEqual(['仕事の案内4の本文']);
  render(<ListingPopup job={job} version={job.versions[0]} onClose={vi.fn()} />);
  const dialog = screen.getByRole('dialog');
  expect(within(dialog).getByText('記載のない任意項目（1件）')).toBeTruthy();
  expect(dialog.querySelector('.jc-listing-popup-missing')?.textContent).not.toMatch(/公開状況|求人名|勤務地|給与/);
  job.listingPublicationStatus = null; job.location = ''; job.title = '求人名未取得';
  expect(listingPopupModel(job, job.versions[0]).missing).toEqual(['仕事の案内4の本文']);
});

it.each(['list-first', 'detail-first'])('shows zero applications from a direct link with %s responses, even in an open popup', async order => {
  let resolveList!: (value: unknown) => void;
  let resolveDetail!: (value: unknown) => void;
  const listResponse = new Promise(resolve => { resolveList = resolve; });
  const detailResponse = new Promise(resolve => { resolveDetail = resolve; });
  api.mockImplementation((path: string) => path.includes('/versions') ? detailResponse : path.includes('/listings?') ? listResponse : Promise.resolve({ ok: false, error: new Error('offline') }));
  window.history.replaceState(null, '', '/app/job-copy?listing=1');
  localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true');
  render(<JobCopyScreen />);
  const listResult = { ok: true, data: { status: 'ready', listings: [{ ...listing, application_count: 0 }], total: 1, offset: 0, next_offset: null, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } };
  const detailResult = { ok: true, data: { listing: { ...listing, application_count: null }, versions: [{ written_at: '2026-10-11T00:00:00Z', body: rowBody({ 案件名: listing.title, 仕事内容: description }), image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } };
  // Wait for both requests to start before deciding which response arrives first.
  await waitFor(() => { expect(api.mock.calls.some(call => String(call[0]).includes('/listings?'))).toBe(true); });
  if (order === 'list-first') await act(async () => { resolveList(listResult); await listResponse; });
  await act(async () => { resolveDetail(detailResult); await detailResponse; });
  fireEvent.click(await screen.findByRole('button', { name: '求人の内容を確認する' }));
  const applications = () => screen.getByRole('dialog').querySelector('.jc-listing-popup-summary')?.textContent;
  if (order === 'detail-first') {
    expect(applications()).toContain('応募数未取得');
    await act(async () => { resolveList(listResult); await listResponse; });
  }
  await waitFor(() => { expect(applications()).toContain('応募数0件'); });
  expect(screen.getByRole('button', { name: `${listing.title}の本文を見る` }).textContent).toContain('応募 0件');
});

it.each([0, 7, null])('uses the detail count %s for a direct link outside the first fifty rows', async count => {
  const rows = Array.from({ length: 50 }, (_, index) => ({ ...listing, id: String(index + 2), title: `架空求人${String(index + 2)}`, application_count: 99 }));
  api.mockImplementation((path: string) => Promise.resolve(path.includes('/versions') ? { ok: true, data: { listing: { ...listing, application_count: count }, versions: [{ written_at: '2026-10-11T00:00:00Z', body: rowBody({ 案件名: listing.title, 仕事内容: description }), image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } } : path.includes('/listings?') ? { ok: true, data: { status: 'ready', listings: rows, total: 51, offset: 0, next_offset: 50, titles: [], index_built_at: '2026-10-11T00:00:00Z', refreshing: false, refresh_failed: false } } : { ok: false, error: new Error('offline') }));
  window.history.replaceState(null, '', '/app/job-copy?listing=1');
  localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true');
  render(<JobCopyScreen />);
  await screen.findByRole('button', { name: '架空求人2の本文を見る' });
  expect(screen.queryByRole('button', { name: `${listing.title}の本文を見る` })).toBeNull();
  fireEvent.click(await screen.findByRole('button', { name: '求人の内容を確認する' }));
  expect(screen.getByRole('dialog').querySelector('.jc-listing-popup-summary')?.textContent).toContain(`応募数${count === null ? '未取得' : `${String(count)}件`}`);
});

it.each([0, 7, null])('refreshes only the acquired count %s after a cold index is ready, retaining the popup and selected body', async count => {
  let ready = false;
  let resolveCount!: (value: unknown) => void;
  const countResponse = new Promise(resolve => { resolveCount = resolve; });
  const rows = Array.from({ length: 50 }, (_, index) => ({ ...listing, id: String(index + 2), title: `架空求人${String(index + 2)}`, application_count: 99 }));
  const latest = rowBody({ 案件名: listing.title, 仕事内容: '現在の本文です。' });
  const earlier = rowBody({ 案件名: listing.title, 仕事内容: '以前の本文です。' });
  api.mockImplementation((path: string) => {
    if (path.endsWith('/applications')) return countResponse;
    if (path.includes('/versions')) return Promise.resolve({ ok: true, data: { listing: { ...listing, application_count: null }, versions: [{ written_at: '2026-10-01T00:00:00Z', body: earlier, image_urls: [] }, { written_at: '2026-10-11T00:00:00Z', body: latest, image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } });
    if (path.includes('/listings?')) return Promise.resolve({ ok: true, data: { status: ready ? 'ready' : 'preparing', listings: ready ? rows : [], total: ready ? 51 : null, offset: 0, next_offset: ready ? 50 : null, titles: [], index_built_at: ready ? '2026-10-11T00:00:00Z' : null, refreshing: !ready, refresh_failed: false } });
    return Promise.resolve({ ok: false, error: new Error('offline') });
  });
  window.history.replaceState(null, '', '/app/job-copy?listing=1');
  localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true');
  render(<JobCopyScreen />);
  await screen.findByText('求人の一覧を準備しています');
  const opener = await screen.findByRole('button', { name: '求人の内容を確認する' });
  fireEvent.click(opener);
  const dialog = screen.getByRole('dialog');
  expect(dialog.querySelector('.jc-listing-popup-summary')?.textContent).toContain('応募数未取得');
  expect(api.mock.calls.filter(call => String(call[0]).endsWith('/applications'))).toHaveLength(0);
  // Finish preparation through the same list reload used by the automatic poll.
  ready = true;
  fireEvent.click(screen.getByRole('button', { name: '求人を読み込む' }));
  await waitFor(() => { expect(api.mock.calls.filter(call => String(call[0]).endsWith('/applications'))).toHaveLength(1); });
  await act(async () => { resolveCount({ ok: true, data: { listing_id: '1', application_count: count } }); await countResponse; });
  expect(screen.getByRole('dialog')).toBe(dialog);
  expect(dialog.querySelector('.jc-listing-popup-summary')?.textContent).toContain(`応募数${count === null ? '未取得' : `${String(count)}件`}`);
  expect(dialog.textContent).toContain('現在の本文です。');
  fireEvent.click(screen.getByRole('button', { name: '求人の内容を閉じる' }));
  fireEvent.click(screen.getByRole('button', { name: /2026\/10\/01 09:00 JST にHubSpotに記録された本文/ }));
  fireEvent.click(opener);
  expect(screen.getByRole('dialog').textContent).toContain('以前の本文です。');
  expect(screen.getByRole('dialog').querySelector('.jc-listing-popup-summary')?.textContent).toContain('応募数未取得');
  fireEvent.click(screen.getByRole('button', { name: '求人の内容を閉じる' }));
  fireEvent.click(screen.getByRole('button', { name: '求人を読み込む' }));
  await waitFor(() => { expect(screen.queryByText('求人の一覧を準備しています')).toBeNull(); });
  expect(api.mock.calls.filter(call => String(call[0]).includes('/versions'))).toHaveLength(1);
  expect(api.mock.calls.filter(call => String(call[0]).endsWith('/applications'))).toHaveLength(1);
});

it('renders exactly two changed rows with colored characters and no mark for unavailable pairs', () => {
  const job = record({ 仕事内容: '商品を確認して梱包します。', 勤務時間: '9:00〜18:00', 応募資格: '' });
  const latest = job.versions[0];
  if (!latest) throw new Error('掲載本文が必要です');
  job.versions.unshift({ ...latest, id: 'previous', observedAt: '2026-10-01T00:00:00Z', comparisonBody: (latest.comparisonBody ?? '').replace('商品を確認して梱包します。', '商品を確認して出荷します。').replace('9:00〜18:00', '8:00〜17:00'), body: '商品を確認して出荷します。' });
  render(<ListingPopup job={job} version={latest} onClose={vi.fn()} />);
  const dialog = screen.getByRole('dialog');
  expect(dialog.querySelectorAll('[data-changed="true"]')).toHaveLength(2);
  expect(dialog.querySelectorAll('[aria-label="変更あり"]')).toHaveLength(4);
  expect([...dialog.querySelectorAll('mark')].map(element => element.textContent).join('')).toContain('梱包');
  fireEvent.click(screen.getByRole('checkbox', { name: '変わった項目だけ表示（2項目）' }));
  expect(dialog.querySelectorAll('.jc-listing-comparison-row')).toHaveLength(2);
  expect(dialog.textContent).not.toContain('記載のない任意項目');
  expect(dialog.textContent).not.toContain('記載なし');
});

it('distinguishes absent images, unresolved filenames and unknown registration, and prefers acquired URLs over empty arrays', () => {
  const job = record({ 画像1: 'warehouse.jpg' });
  expect(listingPopupModel(job, job.versions[0]).imageReason).toBe('画像はありますが、取得できていません');
  job.currentImageObservation = { observedAt: '2026-10-11T01:00:00Z', images: [{ id: 'photo', url: '/api/job-copy/image?sample=1', caption: '職場' }] };
  expect(listingPopupModel(job, job.versions[0]).observedImages?.images[0]?.url).toBe('/api/job-copy/image?sample=1');
  const url = record({ 画像1: 'https://example.invalid/warehouse.jpg' });
  expect(listingPopupModel(url, url.versions[0]).images?.[0]?.url).toBe('https://example.invalid/warehouse.jpg');
  const none = record({ 画像1: 'なし', 画像2: '未登録' });
  expect(listingPopupModel(none, none.versions[0]).imageReason).toBe('この求人には画像が登録されていません');
  expect(listingPopupModel({ ...none, versions: [] }).imageReason).toBe('画像の登録状況を取得できていません');
});

it('switches in the supplied order, uses the latest published copy, and ignores arrows in inputs', async () => {
  const first = record({ 案件名: '最初の求人', 仕事内容: '最初の本文です。' });
  const second = { ...record({ 案件名: '次の求人', 仕事内容: '最新の本文です。' }), id: 'second' };
  const published = second.versions[0];
  if (!published) throw new Error('掲載本文が必要です');
  second.versions.push({ ...published, id: 'draft', kind: 'ai_draft', comparisonBody: rowBody({ 案件名: '未掲載の案' }), body: '未掲載の案です。' });
  const load = vi.fn((id: string) => Promise.resolve(id === first.id ? first : second));
  render(<ListingPopup job={second} version={second.versions[0]} navigation={{ ids: [first.id, second.id], load }} onClose={vi.fn()} />);
  const dialog = screen.getByRole('dialog');
  expect(screen.getByText('2 / 2件目')).toBeTruthy();
  expect(screen.getByRole('button', { name: '次の求人 ›' })).toHaveProperty('disabled', true);
  fireEvent.keyDown(dialog, { key: 'ArrowLeft' });
  await screen.findByRole('heading', { name: '最初の求人' });
  expect(screen.getByRole('dialog')).toBe(dialog);
  expect(screen.getByText('1 / 2件目')).toBeTruthy();
  expect(screen.getByRole('button', { name: '‹ 前の求人' })).toHaveProperty('disabled', true);
  const input = document.createElement('input'); dialog.append(input);
  fireEvent.keyDown(input, { key: 'ArrowRight' }); expect(load).toHaveBeenCalledTimes(1); input.remove();
  fireEvent.keyDown(dialog, { key: 'ArrowRight' });
  await screen.findByRole('heading', { name: '次の求人' });
  expect(dialog.textContent).toContain('最新の本文です。');
  expect(dialog.textContent).not.toContain('未掲載の案です。');
});

it('does not mark unknown historical conditions and shows changed image references in the changes filter', () => {
  const aw = listingRecord({ listing: { ...listing, media: 'airwork' }, current_fields: { 勤務時間: '9:00〜18:00' }, versions: [
    { written_at: '2026-10-01T00:00:00Z', body: '荷物を確認します。', image_urls: null },
    { written_at: '2026-10-11T00:00:00Z', body: '荷物を確認します。', image_urls: null },
  ], history_counts: {}, history_may_be_incomplete: false });
  const { unmount } = render(<ListingPopup job={aw} version={aw.versions[1]} onClose={vi.fn()} />);
  expect(screen.getByRole('dialog').querySelectorAll('[aria-label="変更あり"]')).toHaveLength(0);
  unmount();
  const job = record({ 画像1: 'new.jpg' });
  const selected = job.versions[0];
  if (!selected) throw new Error('掲載本文が必要です');
  job.versions.unshift({ ...selected, id: 'before-image', comparisonBody: (selected.comparisonBody ?? '').replace('new.jpg', 'old.jpg') });
  render(<ListingPopup job={job} version={selected} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole('checkbox', { name: '変わった項目だけ表示（1項目）' }));
  expect(screen.getByRole('dialog').querySelectorAll('[aria-label="変更あり"]')).toHaveLength(2);
  expect(screen.getAllByText('画像はありますが、取得できていません')).toHaveLength(2);
  expect(screen.getByRole('dialog').textContent).not.toMatch(/new.jpg|old.jpg/);
});

it('keeps the current copy while loading and after a failed switch, then permits retry', async () => {
  const job = record({ 案件名: '確認中の求人' });
  let reject!: (reason: Error) => void;
  const pending = new Promise<JobCopyRecord>((_, fail) => { reject = fail; });
  const load = vi.fn(() => pending);
  render(<ListingPopup job={job} version={job.versions[0]} navigation={{ ids: [job.id, 'next'], load }} onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: '次の求人 ›' }));
  expect(screen.getByRole('heading', { name: '確認中の求人' })).toBeTruthy();
  expect(screen.getByRole('button', { name: '次の求人 ›' })).toHaveProperty('disabled', true);
  await act(async () => { reject(new Error('offline')); await Promise.resolve(); });
  expect(screen.getByRole('alert').textContent).toBe('切り替え先の求人を取得できませんでした。もう一度切り替えてください。');
  expect(screen.getByRole('heading', { name: '確認中の求人' })).toBeTruthy();
  expect(screen.getByRole('button', { name: '次の求人 ›' })).toHaveProperty('disabled', false);
});
