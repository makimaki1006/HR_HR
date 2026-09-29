// @vitest-environment happy-dom
import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
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
});
