// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DataTable, ROW_HEIGHT_PX, type DataTableColumn } from './DataTable';

interface Row {
  id: string;
  city: string;
  count: number | null;
}

const columns: DataTableColumn<Row>[] = [
  { key: 'city', header: '市区町村', sortable: true },
  { key: 'count', header: '件数', align: 'right', sortable: true, render: (r) => r.count?.toLocaleString('ja-JP') ?? '-' },
];

function makeRows(n: number): Row[] {
  return Array.from({ length: n }, (_, i) => ({ id: `r${String(i)}`, city: `city${String(i)}`, count: i * 10 }));
}

afterEach(cleanup);

function bodyRows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>('tbody tr:not([data-spacer])'));
}

describe('DataTable', () => {
  it('renders every row of a small table with caption, custom render and alignment', () => {
    const { container } = render(
      <DataTable columns={columns} rows={makeRows(3)} rowKey={(r) => r.id} caption="求人数" />,
    );
    expect(container.querySelector('caption')?.textContent).toBe('求人数');
    const rows = bodyRows(container);
    expect(rows).toHaveLength(3);
    expect(Array.from(rows[2]?.querySelectorAll('td') ?? []).map((td) => td.textContent)).toEqual(['city2', '20']);
    expect(rows[2]?.querySelectorAll('td')[1]?.style.textAlign).toBe('right');
    expect(container.querySelector('[data-virtualized]')).toBeNull();
  });

  it('shows emptyText (default データなし) for zero rows', () => {
    const empty = render(<DataTable columns={columns} rows={[]} rowKey={(r) => r.id} />);
    expect(empty.container.querySelector('tbody td')?.textContent).toBe('データなし');
    cleanup();
    const custom = render(<DataTable columns={columns} rows={[]} rowKey={(r) => r.id} emptyText="該当なし" />);
    expect(custom.container.querySelector('tbody td')?.textContent).toBe('該当なし');
  });

  it('renders all 500 rows without virtualization (threshold is exclusive)', () => {
    const { container } = render(<DataTable columns={columns} rows={makeRows(500)} rowKey={(r) => r.id} />);
    expect(bodyRows(container)).toHaveLength(500);
  });

  it('virtualizes 1000 rows: far fewer than 1000 tr, spacer keeps total height', () => {
    const { container } = render(<DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />);
    const rows = bodyRows(container);
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(100);
    expect(rows[0]?.textContent).toContain('city0');
    const bottom = container.querySelector<HTMLElement>('[data-spacer="bottom"]');
    expect(bottom?.style.height).toBe(`${String((1000 - rows.length) * ROW_HEIGHT_PX)}px`);
    expect(container.querySelector('[data-spacer="top"]')).toBeNull();
  });

  it('shifts the rendered window when scrolled', () => {
    const { container } = render(<DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />);
    const scroller = container.querySelector<HTMLElement>('[data-virtualized]');
    if (!scroller) throw new Error('scroller missing');
    scroller.scrollTop = 500 * ROW_HEIGHT_PX;
    fireEvent.scroll(scroller);
    const rows = bodyRows(container);
    expect(rows.length).toBeLessThan(100);
    const texts = rows.map((r) => r.textContent);
    expect(texts.some((t) => t.startsWith('city500'))).toBe(true);
    expect(texts.some((t) => t.startsWith('city0'))).toBe(false);
    expect(container.querySelector('[data-spacer="top"]')).not.toBeNull();
  });

  it('sorts by a sortable column: asc, desc, then back to original order, nulls last', () => {
    const rows: Row[] = [
      { id: 'a', city: 'B市', count: 5 },
      { id: 'b', city: 'A市', count: null },
      { id: 'c', city: 'C市', count: 30 },
    ];
    const { container, getByRole } = render(<DataTable columns={columns} rows={rows} rowKey={(r) => r.id} />);
    const cities = (): string[] => bodyRows(container).map((r) => r.querySelectorAll('td')[0]?.textContent ?? '');
    const header = getByRole('button', { name: /件数/ });

    expect(cities()).toEqual(['B市', 'A市', 'C市']);
    fireEvent.click(header);
    expect(cities()).toEqual(['B市', 'C市', 'A市']);
    expect(container.querySelectorAll('th')[1]?.getAttribute('aria-sort')).toBe('ascending');
    fireEvent.click(getByRole('button', { name: /件数/ }));
    expect(cities()).toEqual(['C市', 'B市', 'A市']);
    fireEvent.click(getByRole('button', { name: /件数/ }));
    expect(cities()).toEqual(['B市', 'A市', 'C市']);
  });

  it('footerRow renders a tfoot with one aligned cell per column key', () => {
    const { container } = render(
      <DataTable
        columns={columns}
        rows={makeRows(3)}
        rowKey={(r) => r.id}
        footerRow={{ city: '合計', count: '60' }}
      />,
    );
    const cells = Array.from(container.querySelectorAll<HTMLElement>('tfoot tr td'));
    expect(cells.map((c) => c.textContent)).toEqual(['合計', '60']);
    expect(cells[1]?.style.textAlign).toBe('right');
    expect(bodyRows(container)).toHaveLength(3);
  });

  it('footer renders a single cell spanning all columns; footerRow wins over footer', () => {
    const a = render(<DataTable columns={columns} rows={makeRows(1)} rowKey={(r) => r.id} footer="出典: HW" />);
    const td = a.container.querySelector('tfoot td');
    expect(td?.textContent).toBe('出典: HW');
    expect(td?.getAttribute('colspan')).toBe('2');
    cleanup();
    const b = render(
      <DataTable columns={columns} rows={makeRows(1)} rowKey={(r) => r.id} footer="x" footerRow={{ city: 'T' }} />,
    );
    expect(Array.from(b.container.querySelectorAll('tfoot td')).map((c) => c.textContent)).toEqual(['T', '']);
  });

  it('no tfoot without footer props', () => {
    const { container } = render(<DataTable columns={columns} rows={makeRows(1)} rowKey={(r) => r.id} />);
    expect(container.querySelector('tfoot')).toBeNull();
  });

  it("virtualize='never' renders all 1000 rows", () => {
    const { container } = render(
      <DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} virtualize="never" />,
    );
    expect(bodyRows(container)).toHaveLength(1000);
    expect(container.querySelector('[data-virtualized]')).toBeNull();
  });

  it('auto: 1000 rows are windowed, all 1000 while printing, windowed again after afterprint', () => {
    const { container } = render(<DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />);
    const windowed = bodyRows(container).length;
    expect(windowed).toBeLessThan(100);
    expect(container.querySelector('[data-virtualized]')).not.toBeNull();
    act(() => {
      window.dispatchEvent(new Event('beforeprint'));
    });
    expect(bodyRows(container)).toHaveLength(1000);
    expect(container.querySelector('[data-virtualized]')).toBeNull();
    act(() => {
      window.dispatchEvent(new Event('afterprint'));
    });
    expect(bodyRows(container)).toHaveLength(windowed);
  });

  it('beforeprint outside act(): all 1000 rows are in the DOM synchronously after dispatchEvent', () => {
    const { container } = render(
      <DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />,
    );
    expect(bodyRows(container).length).toBeLessThan(100);
    // No act(): the page is printed right after the beforeprint listeners return.
    window.dispatchEvent(new Event('beforeprint'));
    expect(bodyRows(container)).toHaveLength(1000);
    window.dispatchEvent(new Event('afterprint'));
  });

  it('matchMedia(print) change to matches:true is flushed synchronously too', () => {
    const listeners: ((e: { matches: boolean }) => void)[] = [];
    vi.stubGlobal('matchMedia', () => ({
      matches: false,
      addEventListener: (_t: string, cb: (e: { matches: boolean }) => void) => listeners.push(cb),
      removeEventListener: () => undefined,
    }));
    try {
      const { container } = render(
        <DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />,
      );
      listeners.forEach((l) => {
        l({ matches: true });
      });
      expect(bodyRows(container)).toHaveLength(1000);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('after printing the window follows the real scrollTop of the new scroll container (0)', () => {
    const { container } = render(
      <DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />,
    );
    const scroller = container.querySelector('[data-virtualized]') as HTMLElement;
    act(() => {
      scroller.scrollTop = 18_000; // row 500
      fireEvent.scroll(scroller);
    });
    expect(container.querySelector('[data-spacer="top"]')).not.toBeNull();
    act(() => {
      window.dispatchEvent(new Event('beforeprint'));
    });
    act(() => {
      window.dispatchEvent(new Event('afterprint'));
    });
    const fresh = container.querySelector('[data-virtualized]') as HTMLElement;
    expect(fresh.scrollTop).toBe(0);
    // Window starts at row 0 again: no top spacer, first row is the first data row.
    expect(container.querySelector('[data-spacer="top"]')).toBeNull();
    expect(bodyRows(container)[0]?.textContent).toContain('city0');
  });

  it('matchMedia(print) change also switches to full rendering', () => {
    const listeners: ((e: { matches: boolean }) => void)[] = [];
    vi.stubGlobal('matchMedia', () => ({
      matches: false,
      addEventListener: (_t: string, cb: (e: { matches: boolean }) => void) => listeners.push(cb),
      removeEventListener: () => undefined,
    }));
    try {
      const { container } = render(<DataTable columns={columns} rows={makeRows(1000)} rowKey={(r) => r.id} />);
      act(() => {
        listeners.forEach((l) => {
          l({ matches: true });
        });
      });
      expect(bodyRows(container)).toHaveLength(1000);
      act(() => {
        listeners.forEach((l) => {
          l({ matches: false });
        });
      });
      expect(bodyRows(container).length).toBeLessThan(100);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
