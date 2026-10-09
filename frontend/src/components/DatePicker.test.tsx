// @vitest-environment happy-dom
import { useState } from 'react';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DatePicker } from './DatePicker';
import { addMonths, monthGrid, parseDate } from './datePickerModel';
import { loadHolidayYear } from './holidays';

afterEach(cleanup);

function Harness({ initial = '2026-05-10', min, onValue }: { initial?: string; min?: string; onValue?: (v: string) => void }) {
  const [v, setV] = useState(initial);
  return <DatePicker aria-label="次回架電日" value={v} min={min} today="2026-05-01"
    onChange={x => { setV(x); onValue?.(x); }} />;
}

const openCal = async () => {
  fireEvent.click(screen.getByRole('button', { name: 'カレンダーを開く' }));
  // 祝日データの遅延読み込みが終わるまで待つ
  await waitFor(() => { expect(cell('2026-05-04').className).toContain('dp-holiday'); });
};
const cell = (iso: string): HTMLButtonElement => {
  const el = document.querySelector<HTMLButtonElement>(`button[data-date="${iso}"]`);
  if (el === null) throw new Error(`cell ${iso} not found`);
  return el;
};

describe('DatePicker', () => {
  it('2026 年の祝日 (振替休日を含む) は赤、土曜は青、平日は色なし', async () => {
    render(<Harness />);
    await openCal();
    for (const [iso, name] of [['2026-05-03', '憲法記念日'], ['2026-05-04', 'みどりの日'], ['2026-05-05', 'こどもの日'], ['2026-05-06', 'こどもの日 振替休日']] as const) {
      expect(cell(iso).className).toContain('dp-holiday');
      expect(cell(iso).getAttribute('aria-label')).toBe(`5月${String(Number(iso.slice(-2)))}日 ${name}`);
      expect(cell(iso).title).toContain(name);
    }
    expect(cell('2026-05-02').className).toContain('dp-sat');
    expect(cell('2026-05-02').className).not.toContain('dp-holiday');
    expect(cell('2026-05-01').className).not.toMatch(/dp-(sat|sun|holiday)/);
    expect(cell('2026-05-10').className).toContain('dp-sun'); // 日曜
    expect(cell('2026-05-01').className).toContain('dp-today');
    expect(cell('2026-05-10').className).toContain('dp-selected');
    // 前後の月 (4/29 昭和の日) は灰色
    expect(cell('2026-04-29').className).toContain('dp-out');
  });

  it('見出しは 2026年(令和8年) 5月、曜日は日〜土', async () => {
    render(<Harness />);
    await openCal();
    expect(screen.getByText('2026年(令和8年) 5月')).toBeTruthy();
    expect(screen.getAllByRole('columnheader').map(h => h.textContent).join('')).toBe('日月火水木金土');
    fireEvent.click(screen.getByRole('button', { name: '次の月' }));
    expect(screen.getByText('2026年(令和8年) 6月')).toBeTruthy();
  });

  it('月をまたぐ・年をまたぐ表示でも祝日が出る (2026-12 の末尾の 2027-01-01 元日)', async () => {
    render(<Harness initial="2026-12-15" />);
    fireEvent.click(screen.getByRole('button', { name: 'カレンダーを開く' }));
    await waitFor(() => { expect(cell('2027-01-01').getAttribute('aria-label')).toBe('1月1日 元日'); });
    expect(cell('2027-01-01').className).toContain('dp-out');
  });

  it('2026/5/21 と打つと 2026-05-21 を渡す。範囲外・不正は渡さない', () => {
    const seen: string[] = [];
    render(<Harness onValue={v => seen.push(v)} />);
    const input = screen.getByLabelText('次回架電日');
    fireEvent.change(input, { target: { value: '2026/5/21' } });
    expect(seen.at(-1)).toBe('2026-05-21');
    fireEvent.change(input, { target: { value: '2026-02-30' } });
    expect(seen.at(-1)).toBe('2026-05-21');
    fireEvent.change(input, { target: { value: '' } });
    expect(seen.at(-1)).toBe('');
  });

  it('日を選ぶと YYYY-MM-DD で渡して閉じ、入力欄に戻る。今日・削除も動く', async () => {
    const seen: string[] = [];
    render(<Harness onValue={v => seen.push(v)} />);
    await openCal();
    fireEvent.click(cell('2026-05-21'));
    expect(seen.at(-1)).toBe('2026-05-21');
    expect(screen.queryByTestId('dp-popover')).toBeNull();
    expect(screen.getByLabelText<HTMLInputElement>('次回架電日').value).toBe('2026/05/21');
    expect(document.activeElement).toBe(screen.getByLabelText('次回架電日'));
    fireEvent.click(screen.getByRole('button', { name: 'カレンダーを開く' }));
    fireEvent.click(screen.getByRole('button', { name: '今日' }));
    expect(seen.at(-1)).toBe('2026-05-01');
    fireEvent.click(screen.getByRole('button', { name: 'カレンダーを開く' }));
    fireEvent.click(screen.getByRole('button', { name: '削除' }));
    expect(seen.at(-1)).toBe('');
    expect(screen.getByLabelText<HTMLInputElement>('次回架電日').value).toBe('');
  });

  it('Esc で閉じて入力欄にフォーカスが戻る。閉じているときの Esc は親へ渡る', async () => {
    const parent = vi.fn();
    render(<DatePicker aria-label="日付" value="2026-05-10" onChange={() => undefined} onKeyDown={e => { parent(e.key); }} />);
    const input = screen.getByLabelText('日付');
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(parent).toHaveBeenCalledWith('Escape');
    parent.mockClear();
    fireEvent.click(screen.getByRole('button', { name: 'カレンダーを開く' }));
    await waitFor(() => { expect(document.activeElement).toBe(cell('2026-05-10')); });
    fireEvent.keyDown(cell('2026-05-10'), { key: 'Escape' });
    expect(screen.queryByTestId('dp-popover')).toBeNull();
    expect(document.activeElement).toBe(input);
    expect(parent).not.toHaveBeenCalled();
  });

  it('矢印で日、PageUp/PageDown で月を動かし、Enter で選ぶ', async () => {
    const seen: string[] = [];
    render(<Harness onValue={v => seen.push(v)} />);
    await openCal();
    const grid = screen.getByRole('grid');
    fireEvent.keyDown(cell('2026-05-10'), { key: 'ArrowRight' });
    await waitFor(() => { expect(document.activeElement).toBe(cell('2026-05-11')); });
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await waitFor(() => { expect(document.activeElement).toBe(cell('2026-05-18')); });
    fireEvent.keyDown(grid, { key: 'PageDown' });
    await waitFor(() => { expect(document.activeElement).toBe(cell('2026-06-18')); });
    expect(screen.getByText('2026年(令和8年) 6月')).toBeTruthy();
    fireEvent.keyDown(grid, { key: 'PageUp' });
    await waitFor(() => { expect(document.activeElement).toBe(cell('2026-05-18')); });
    fireEvent.click(document.activeElement as HTMLElement); // Enter はボタンの click になる
    expect(seen.at(-1)).toBe('2026-05-18');
  });

  it('min より前の日は選べない (aria-disabled、クリックしても値は変わらない)', async () => {
    const seen: string[] = [];
    render(<Harness min="2026-05-12" onValue={v => seen.push(v)} />);
    await openCal();
    expect(cell('2026-05-11').getAttribute('aria-disabled')).toBe('true');
    expect(cell('2026-05-12').getAttribute('aria-disabled')).toBeNull();
    fireEvent.click(cell('2026-05-11'));
    expect(seen).toEqual([]);
    expect(screen.getByTestId('dp-popover')).toBeTruthy();
    // 今日 (5/1) が min より前なので「今日」も押せない
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '今日' }).disabled).toBe(true);
  });
});

describe('日付の計算と祝日データ', () => {
  it('parseDate は実在する日だけ受け付ける', () => {
    expect(parseDate('2026/5/21')).toBe('2026-05-21');
    expect(parseDate('2026-05-21')).toBe('2026-05-21');
    expect(parseDate('２０２６/５/２１')).toBe('2026-05-21');
    expect(parseDate('2026-02-29')).toBeNull();
    expect(parseDate('2026-13-01')).toBeNull();
    expect(parseDate('2026/5')).toBeNull();
  });
  it('addMonths は月末を丸める。monthGrid は日曜始まりの 42 日', () => {
    expect(addMonths('2026-01-31', 1)).toBe('2026-02-28');
    const g = monthGrid(2026, 5);
    expect(g).toHaveLength(42);
    expect(g[0]).toBe('2026-04-26'); // 2026-05-01 は金曜
  });
  it('2026 年は 18 日 (祝日 + 振替休日 + 国民の休日)、範囲外の年は空', async () => {
    const h = await loadHolidayYear(2026);
    expect(Object.keys(h)).toHaveLength(18);
    expect(h['2026-09-22']).toBe('国民の休日');
    expect(h['2026-05-06']).toBe('こどもの日 振替休日');
    expect(await loadHolidayYear(1999)).toEqual({});
    expect(await loadHolidayYear(2100)).toEqual({});
  });
});
