import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';

export interface DataTableColumn<T> {
  key: string;
  header: string;
  render?: (row: T) => ReactNode;
  align?: 'left' | 'right' | 'center';
  sortable?: boolean;
  /** Sort key; defaults to `row[key]`. */
  sortValue?: (row: T) => string | number | null;
}

export interface DataTableProps<T> {
  columns: readonly DataTableColumn<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  caption?: string;
  emptyText?: string;
  /** Free-form tfoot content spanning all columns. Ignored when footerRow is given. */
  footer?: ReactNode;
  /** Totals row: one cell per column, looked up by column key (aligned like the column). */
  footerRow?: Partial<Record<string, ReactNode>>;
  /** 'auto' (default): windowed above VIRTUALIZE_THRESHOLD rows, but all rows while printing. */
  virtualize?: 'auto' | 'never';
}

/** Above this many rows the body is windowed (only visible rows are in the DOM). */
export const VIRTUALIZE_THRESHOLD = 500;
export const ROW_HEIGHT_PX = 36;
export const VIEWPORT_HEIGHT_PX = 480;
const OVERSCAN_ROWS = 8;

type SortState = { key: string; dir: 'asc' | 'desc' } | null;

const stickyHead = { position: 'sticky', top: 0 } as const;

function rawValue(row: unknown, key: string): unknown {
  return (row as Record<string, unknown>)[key];
}

function cellValue<T>(row: T, col: DataTableColumn<T>): string | number | null {
  if (col.sortValue) return col.sortValue(row);
  const v = rawValue(row, col.key);
  return typeof v === 'number' || typeof v === 'string' ? v : null;
}

function compareValues(a: string | number, b: string | number): number {
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  return String(a).localeCompare(String(b), 'ja');
}

function cellContent<T>(row: T, col: DataTableColumn<T>): ReactNode {
  if (col.render) return col.render(row);
  const v = rawValue(row, col.key);
  return typeof v === 'string' || typeof v === 'number' ? String(v) : '';
}

/**
 * True between beforeprint and afterprint (also follows matchMedia('print')).
 * The browser prints right after beforeprint returns, so the switch to "all rows" is flushed
 * synchronously (flushSync) instead of being left to React's batching.
 */
function usePrinting(): boolean {
  const [printing, setPrinting] = useState(false);
  useEffect(() => {
    const on = (): void => {
      flushSync(() => {
        setPrinting(true);
      });
    };
    const off = (): void => {
      setPrinting(false);
    };
    window.addEventListener('beforeprint', on);
    window.addEventListener('afterprint', off);
    const query = typeof window.matchMedia === 'function' ? window.matchMedia('print') : null;
    const onChange = (e: MediaQueryListEvent): void => {
      if (e.matches) {
        flushSync(() => {
          setPrinting(true);
        });
      } else {
        setPrinting(false);
      }
    };
    query?.addEventListener('change', onChange);
    return () => {
      window.removeEventListener('beforeprint', on);
      window.removeEventListener('afterprint', off);
      query?.removeEventListener('change', onChange);
    };
  }, []);
  return printing;
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  caption,
  emptyText,
  footer,
  footerRow,
  virtualize = 'auto',
}: DataTableProps<T>) {
  const [sort, setSort] = useState<SortState>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const printing = usePrinting();
  const scrollRef = useRef<HTMLDivElement>(null);

  const sorted = useMemo(() => {
    if (sort === null) return rows;
    const col = columns.find((c) => c.key === sort.key);
    if (!col) return rows;
    const sign = sort.dir === 'asc' ? 1 : -1;
    return [...rows].sort((x, y) => {
      const a = cellValue(x, col);
      const b = cellValue(y, col);
      // Missing values stay last in both directions.
      if (a === null || b === null) return a === b ? 0 : a === null ? 1 : -1;
      return sign * compareValues(a, b);
    });
  }, [rows, columns, sort]);

  const toggleSort = (key: string): void => {
    setSort((cur) => {
      if (cur?.key !== key) return { key, dir: 'asc' };
      return cur.dir === 'asc' ? { key, dir: 'desc' } : null;
    });
  };

  const virtual = virtualize === 'auto' && !printing && sorted.length > VIRTUALIZE_THRESHOLD;
  // The scroll container is re-created after printing (or when rows grow past the threshold):
  // take its real scrollTop so the window matches what is on screen.
  useLayoutEffect(() => {
    setScrollTop(virtual && scrollRef.current ? scrollRef.current.scrollTop : 0);
  }, [virtual]);
  let start = 0;
  let end = sorted.length;
  if (virtual) {
    const visible = Math.ceil(VIEWPORT_HEIGHT_PX / ROW_HEIGHT_PX);
    start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT_PX) - OVERSCAN_ROWS);
    end = Math.min(sorted.length, start + visible + OVERSCAN_ROWS * 2);
  }
  const shown = sorted.slice(start, end);
  const padTop = start * ROW_HEIGHT_PX;
  const padBottom = (sorted.length - end) * ROW_HEIGHT_PX;

  const table = (
    <table className="hw-data-table">
      {caption === undefined ? null : <caption>{caption}</caption>}
      <thead>
        <tr>
          {columns.map((col) => {
            const active = sort?.key === col.key ? sort.dir : null;
            return (
              <th
                key={col.key}
                scope="col"
                style={{ textAlign: col.align ?? 'left', ...(virtual ? stickyHead : {}) }}
                aria-sort={
                  active === 'asc' ? 'ascending' : active === 'desc' ? 'descending' : undefined
                }
              >
                {col.sortable === true ? (
                  <button
                    type="button"
                    onClick={() => {
                      toggleSort(col.key);
                    }}
                  >
                    {col.header}
                    {active === 'asc' ? ' ▲' : active === 'desc' ? ' ▼' : ''}
                  </button>
                ) : (
                  col.header
                )}
              </th>
            );
          })}
        </tr>
      </thead>
      <tbody>
        {sorted.length === 0 ? (
          <tr>
            <td colSpan={columns.length} className="hw-data-table-empty">
              {emptyText ?? 'データなし'}
            </td>
          </tr>
        ) : (
          <>
            {padTop > 0 ? <tr aria-hidden="true" data-spacer="top" style={{ height: padTop }} /> : null}
            {shown.map((row) => (
              <tr key={rowKey(row)} style={virtual ? { height: ROW_HEIGHT_PX } : undefined}>
                {columns.map((col) => (
                  <td key={col.key} style={{ textAlign: col.align ?? 'left' }}>
                    {cellContent(row, col)}
                  </td>
                ))}
              </tr>
            ))}
            {padBottom > 0 ? (
              <tr aria-hidden="true" data-spacer="bottom" style={{ height: padBottom }} />
            ) : null}
          </>
        )}
      </tbody>
      {footerRow !== undefined ? (
        <tfoot>
          <tr>
            {columns.map((col) => (
              <td key={col.key} style={{ textAlign: col.align ?? 'left' }}>
                {footerRow[col.key]}
              </td>
            ))}
          </tr>
        </tfoot>
      ) : footer !== undefined ? (
        <tfoot>
          <tr>
            <td colSpan={columns.length}>{footer}</td>
          </tr>
        </tfoot>
      ) : null}
    </table>
  );

  if (!virtual) return table;
  return (
    <div
      ref={scrollRef}
      className="hw-data-table-scroll"
      data-virtualized="true"
      style={{ height: VIEWPORT_HEIGHT_PX, overflowY: 'auto' }}
      onScroll={(e) => {
        setScrollTop(e.currentTarget.scrollTop);
      }}
    >
      {table}
    </div>
  );
}
