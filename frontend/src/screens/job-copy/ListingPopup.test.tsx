// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
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
  expect(model.missing).toEqual(['仕事の案内4の本文', '応募に必要な資格・経験', '勤務時間']);
  expect(model.sections.find(section => section.heading === 'その他')?.entries).toContainEqual({ key: 'unmapped_field', label: 'その他の案内', value: '落としてはいけない案内' });
  render(<ListingPopup job={job} version={job.versions[0]} onClose={vi.fn()} />);
  expect(screen.getByRole('heading', { name: '未取得の項目（3件）' })).toBeTruthy();
  expect(screen.getByRole('dialog').textContent).not.toContain('unmapped_field');
});
it('keeps numeric codes as review-needed, and never turns missing salaries or counts into zero', () => {
  const job = record({ 給与形態: '01', 雇用形態: '02', '試用・研修の有無': '1', 公開: '01' });
  const model = listingPopupModel(job, job.versions[0]);
  expect(model.salary).toBe('確認が必要 213,000円〜249,000円');
  expect(model.sections.find(section => section.heading === 'その他')?.entries.find(item => item.key === '雇用形態')?.value).toBe('確認が必要');
  expect(model.publication).toBe('確認が必要');
  const missing = record({ 給与形態: '', '基本給与 最小': '', '基本給与 最大': '' }); missing.listingApplicationCount = null;
  expect(listingPopupModel(missing, missing.versions[0])).toMatchObject({ salary: '未取得', applications: '未取得' });
  missing.listingApplicationCount = 0;
  expect(listingPopupModel(missing, missing.versions[0]).applications).toBe('0件');
});
it('shows AirWork independent conditions only with its latest body, retaining historical full text', () => {
  const job = listingRecord({ listing: { ...listing, media: 'airwork' }, current_fields: { 案件名: listing.title, 仕事内容: '現在の仕事内容。', 給与形態: '月給', 勤務地: '東京都江東区', 勤務時間: '8:00〜17:00', '休日・休暇': '土日休み', 福利厚生: '社会保険', 雇用形態: '正社員' }, versions: [
    { written_at: '2026-10-01T00:00:00Z', body: '過去の仕事内容。\n最後の文章。', image_urls: null },
    { written_at: '2026-10-11T00:00:00Z', body: '現在の仕事内容。', image_urls: null },
  ], history_counts: { shigotonaiyou: 2 }, history_may_be_incomplete: false });
  const old = listingPopupModel(job, job.versions[0]);
  expect(old.sections).toEqual([{ heading: '仕事内容', entries: [{ key: '仕事内容', label: '仕事内容', value: '過去の仕事内容。\n最後の文章。' }] }]);
  expect(old).toMatchObject({ location: '未取得', publication: '未取得', hasCurrentConditions: false });
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
  expect(model.sections.map(section => section.heading)).toEqual(['勤務地・アクセス', '福利厚生']);
  expect(model.missing).toContain('仕事内容');
  expect(model.missing).not.toContain('画像');
  expect(model.observedImages?.observedAt).toBe('2026-10-11T01:00:00Z');
  expect(model).toMatchObject({ publication: '公開', applications: '12件', salary: '未取得' });
});
