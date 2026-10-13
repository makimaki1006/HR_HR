// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { FullTextCompareDialog } from './FullTextCompareDialog';
import type { CopyVersion, JobCopyRecord } from './data';

beforeEach(() => {
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (this: HTMLDialogElement) { this.setAttribute('open', ''); });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (this: HTMLDialogElement) { this.removeAttribute('open'); });
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); document.body.style.overflow = ''; });

const version = (id: string, label: string, observedAt: string, sections: { heading: string; text: string }[]): CopyVersion => ({
  id, label, observedAt, certainty: 'confirmed', kind: 'published', source: 'テスト', applications: null, note: '',
  body: sections.map(section => `${section.heading}：${section.text}`).join('\n'), bodySections: sections,
});
const base = [{ heading: '仕事内容', text: '荷物を運びます。' }, { heading: '給与', text: '月給 250,000円' }, { heading: '勤務時間', text: '8:00〜17:00' }];
const record = (media = 'HRハッカー'): JobCopyRecord => ({
  id: 'r1', title: '架空の配送スタッフ', company: '架空運輸', media, mediaJobId: 'X', location: '東京都', versions: [
    version('v1', '初回', '2026-09-01T00:00:00Z', base),
    version('v2', '変更後', '2026-09-15T00:00:00Z', [{ heading: '仕事内容', text: '荷物を運び、伝票を整理します。' }, { heading: '給与', text: '月給 270,000円' }, base[2] as { heading: string; text: string }]),
    version('v3', '最新', '2026-09-20T00:00:00Z', [{ heading: '仕事内容', text: '荷物を運び、伝票を整理します。' }, { heading: '給与', text: '月給 270,000円' }, base[2] as { heading: string; text: string }]),
  ],
});

it('shows only the 2 differing items by default and all items in 全文', () => {
  render(<FullTextCompareDialog record={record()} open onClose={vi.fn()} initialLeft="v1" initialRight="v2" />);
  const dialog = screen.getByRole('dialog');
  expect(dialog.getAttribute('aria-modal')).toBe('true');
  expect(screen.getByRole('status').textContent).toContain('全 3 項目のうち 2 項目が違います。');
  expect(dialog.querySelectorAll('.jc-ftc-row')).toHaveLength(2);
  expect(dialog.querySelector('.jc-ftc-rows')?.textContent).not.toContain('勤務時間');
  expect(dialog.textContent).toContain('掲載開始時の内容');
  expect(dialog.textContent).toContain('2026/09/01');
  fireEvent.click(screen.getByRole('button', { name: '全文' }));
  expect(dialog.querySelectorAll('.jc-ftc-row')).toHaveLength(4); // 3 items + 画像
  expect(dialog.textContent).toContain('8:00〜17:00');
});

it('switches versions and updates the result and the change range', () => {
  render(<FullTextCompareDialog record={record()} open onClose={vi.fn()} />);
  // default: second-latest (v2) vs latest (v3): identical
  expect(screen.getByRole('status').textContent).toContain('全 3 項目のうち 0 項目が違います。');
  expect(screen.getByText('違う項目はありません。')).toBeTruthy();
  const left = screen.getByRole('group', { name: '左（前）' });
  fireEvent.click(within(left).getByRole('button', { name: /初回/ }));
  expect(screen.getByRole('status').textContent).toContain('全 3 項目のうち 2 項目が違います。');
  const right = screen.getByRole('group', { name: '右（後）' });
  fireEvent.click(within(right).getByRole('button', { name: /変更後/ }));
  expect(screen.getByRole('dialog').textContent).toContain('2026/09/01 09:00 JST〜2026/09/15 09:00 JST の間に変わった内容');
});

it('says 記録していません for items AirWork does not keep, without counting them as equal', () => {
  const air = record('AirWork');
  air.versions = [version('a1', '初回', '2026-09-01T00:00:00Z', [{ heading: '仕事内容', text: '旧' }]), version('a2', '次', '2026-09-10T00:00:00Z', [{ heading: '仕事内容', text: '新' }])];
  render(<FullTextCompareDialog record={air} open onClose={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toContain('全 1 項目のうち 1 項目が違います。');
  fireEvent.click(screen.getByRole('button', { name: '全文' }));
  expect(screen.getAllByText('記録していません').length).toBeGreaterThanOrEqual(8);
});

it('closes with Escape', () => {
  const onClose = vi.fn();
  render(<FullTextCompareDialog record={record()} open onClose={onClose} />);
  fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
  expect(onClose).toHaveBeenCalledTimes(1);
});

it('renders nothing when closed', () => {
  render(<FullTextCompareDialog record={record()} open={false} onClose={vi.fn()} />);
  expect(screen.queryByRole('dialog')).toBeNull();
});

const jobTypeVersions = (): CopyVersion[] => [version('p1', '前', '2026-09-01T00:00:00Z', [{ heading: '職種', text: 'a' }, { heading: 'Indeed表示職種名', text: 'b' }]), version('p2', '後', '2026-09-10T00:00:00Z', [{ heading: '職種', text: 'a' }])];
const rowOf = (label: string) => [...document.querySelectorAll('.jc-ftc-row')].find(row => row.querySelector('.jc-ftc-label')?.textContent.startsWith(label));

it('HRハッカー: 職種 and Indeed表示職種名 are separate; a heading removed on one side is a real change', () => {
  const r = record(); r.versions = jobTypeVersions();
  render(<FullTextCompareDialog record={r} open onClose={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toContain('全 2 項目のうち 1 項目が違います。');
  expect(rowOf('Indeed表示職種名')?.getAttribute('data-changed')).toBe('true');
  expect(rowOf('Indeed表示職種名')?.textContent).toContain('変わった');
  expect(rowOf('Indeed表示職種名')?.textContent).toContain('（記載なし）');
  expect(rowOf('職種')).toBeUndefined();
  fireEvent.click(screen.getByRole('button', { name: '全文' }));
  expect(rowOf('職種')?.getAttribute('data-changed')).toBe('false');
});

it('AirWork: a heading missing on one side is 記録していません and not counted as different', () => {
  const r = record('AirWork'); r.versions = jobTypeVersions();
  render(<FullTextCompareDialog record={r} open onClose={vi.fn()} />);
  expect(screen.getByRole('status').textContent).toContain('違います。');
  expect(screen.getByText('違う項目はありません。')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '全文' }));
  expect(rowOf('Indeed表示職種名')?.textContent).toContain('記録していません');
  expect(rowOf('Indeed表示職種名')?.getAttribute('data-changed')).toBe('false');
});
