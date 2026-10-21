// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(cleanup);
import { PageNav, SortHeader } from './SharedTable';
import { pageCount, paginate, sortArrow, sortRows } from './sharedTableModel';

interface Row { id: string; name: string | null; n: number | null }
const rows: Row[] = [{ id: 'c', name: 'ろ', n: 5 }, { id: 'a', name: null, n: null }, { id: 'b', name: 'い', n: 5 }, { id: 'd', name: 'は', n: 1 }];

describe('sortRows', () => {
  it('数は昇順・降順で並び、空はどちらでも最後、同じ値は鍵の順', () => {
    expect(sortRows(rows, row => row.n, false, undefined, row => row.id).map(row => row.id)).toEqual(['d', 'b', 'c', 'a']);
    expect(sortRows(rows, row => row.n, true, undefined, row => row.id).map(row => row.id)).toEqual(['b', 'c', 'd', 'a']);
  });
  it('文字は日本語順で、同じ値のときは追加の比べ方が先に効く', () => {
    expect(sortRows(rows, row => row.name, false).map(row => row.name)).toEqual(['い', 'は', 'ろ', null]);
    expect(sortRows(rows, row => row.n, true, (a, b) => b.id.localeCompare(a.id)).map(row => row.id)).toEqual(['c', 'b', 'd', 'a']);
  });
  it('元の配列は変えない', () => { const before = rows.map(row => row.id); sortRows(rows, row => row.n, true); expect(rows.map(row => row.id)).toEqual(before); });
});

describe('paginate', () => {
  const items = Array.from({ length: 45 }, (_, i) => i + 1);
  it('1ページ目と最後のページの件数と範囲', () => {
    const first = paginate(items, 0, 20); expect(first.items).toHaveLength(20); expect([first.from, first.to, first.pages]).toEqual([1, 20, 3]);
    const last = paginate(items, 2, 20); expect(last.items).toEqual([41, 42, 43, 44, 45]); expect([last.from, last.to]).toEqual([41, 45]);
  });
  it('範囲外は端に寄せ、0件は 1 ページ・範囲 0', () => {
    expect(paginate(items, 9, 20).pageIndex).toBe(2); expect(paginate(items, -3, 20).pageIndex).toBe(0);
    expect(paginate([], 0, 20)).toMatchObject({ items: [], pages: 1, from: 0, to: 0 });
    expect(pageCount(40, 20)).toBe(2); expect(pageCount(41, 20)).toBe(3);
  });
});

describe('SortHeader と PageNav', () => {
  it('選んだ列だけ向きを出し、押すと並べ替えを呼ぶ', () => {
    expect(sortArrow(true, true)).toBe(' ▼'); expect(sortArrow(true, false)).toBe(' ▲'); expect(sortArrow(false, true)).toBe(' ▲▼');
    const onSort = vi.fn();
    render(<table><thead><tr><SortHeader label="満了まで" active descending={false} onSort={onSort} /><SortHeader label="担当" active={false} descending onSort={onSort} /></tr></thead></table>);
    const [a, b] = screen.getAllByRole('columnheader');
    expect(a?.getAttribute('aria-sort')).toBe('ascending'); expect(a?.textContent).toBe('満了まで ▲');
    expect(b?.getAttribute('aria-sort')).toBe('none'); expect(b?.textContent).toBe('担当 ▲▼');
    fireEvent.click(screen.getByRole('button', { name: /^担当/ })); expect(onSort).toHaveBeenCalledTimes(1);
  });
  it('端のページでは前／次が押せず、押すと対応する関数を呼ぶ', () => {
    const prev = vi.fn(); const next = vi.fn();
    const { rerender } = render(<PageNav className="x" label="テストのページ" size={20} hasPrev={false} hasNext onPrev={prev} onNext={next}>1 / 3</PageNav>);
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '前の20件' }).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '次の20件' })); expect(next).toHaveBeenCalledTimes(1); expect(prev).not.toHaveBeenCalled();
    expect(screen.getByRole('navigation', { name: 'テストのページ' }).textContent).toBe('前の20件1 / 3次の20件');
    rerender(<PageNav className="x" label="テストのページ" size={20} hasPrev hasNext={false} onPrev={prev} onNext={next}>3 / 3</PageNav>);
    expect(screen.getByRole<HTMLButtonElement>('button', { name: '次の20件' }).disabled).toBe(true);
    fireEvent.click(screen.getByRole('button', { name: '前の20件' })); expect(prev).toHaveBeenCalledTimes(1);
  });
});
