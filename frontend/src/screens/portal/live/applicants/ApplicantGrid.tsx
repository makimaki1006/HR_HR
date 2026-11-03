import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { PortalSheetRows } from '../../../../generated/PortalSheetRows';
import { Grid } from '../../customer/list/Grid';
import { matchesFilters, orderColumns, omit, sortRows, type ColumnFilter, type SortSpec } from '../../customer/list/model';
import { ToastBar } from '../../customer/list/parts';
import { useCellSaver, type SaveCell } from '../../customer/list/useCellSaver';
import type { ApplicantRow, SheetConfig } from '../../types';
import { columnConfig, type EditableApplicant, type SaveRow } from './editing';
import { listLayout, matches } from './model';

/** 試作と同じ表を、顧客APIから受け取った値だけで描く。試作のストアには接続しない。 */
export function ApplicantGrid({ data, selected, query, onOpen, saveRow }: {
  data: PortalSheetRows;
  selected: number | null;
  query: string;
  onOpen: (row: number) => void;
  saveRow: SaveRow;
}) {
  const [sort, setSort] = useState<SortSpec | null>(null);
  const [filters, setFilters] = useState<Record<string, ColumnFilter>>({});
  const bases = useRef<Record<string, EditableApplicant>>({});
  const dataRef = useRef(data);
  useLayoutEffect(() => { dataRef.current = data; }, [data]);
  const layout = listLayout(data.columns);
  const columns = useMemo(() => data.columns.map(columnConfig), [data.columns]);
  const titleHeader = columns[layout.title ?? 0]?.header ?? '';
  const statusHeader = layout.status === null ? undefined : columns[layout.status]?.header;
  const sheet = useMemo<SheetConfig>(() => ({
    id: data.sheet.id, customerId: '', tabName: data.sheet.label, headerRow: 1,
    columns, titleColumn: titleHeader, connection: 'ok',
    ...(statusHeader ? { statusColumn: statusHeader } : {}),
  }), [data.sheet.id, data.sheet.label, columns, titleHeader, statusHeader]);
  const allRows = useMemo<ApplicantRow[]>(() => data.applicants.map((row) => ({
    rowNumber: row.row,
    values: Object.fromEntries(columns.map((column, index) => [column.header, row.cells[index] ?? ''])),
    changedSinceLastView: null,
  })), [data.applicants, columns]);
  const visible = useMemo(() => new Set(data.applicants.filter((row) => matches(row, query)).map((row) => row.row)), [data.applicants, query]);
  const rows = useMemo(() => sortRows(allRows.filter((row) => visible.has(row.rowNumber) && matchesFilters(row, filters)), sort, undefined), [allRows, visible, filters, sort]);
  const cols = orderColumns(columns.filter((column) => column.inList), titleHeader, statusHeader);

  const saveCell = useCallback<SaveCell>(async (_sheet, rowNumber, column, base, next) => {
    const key = `${String(rowNumber)}|${column}`;
    const current = dataRef.current;
    const index = current.columns.findIndex((item) => columnConfig(item).header === column);
    const row = bases.current[key] ?? current.applicants.find((item) => item.row === rowNumber);
    if (!row || index < 0 || row.cells[index] !== base) return { kind: 'failed', message: '行の内容が変わっています。詳細を開き直してからお試しください' };
    const result = await saveRow(row, [{ column, value: next }]);
    if (result.kind === 'saved') {
      bases.current = omit(bases.current, key);
      return { kind: 'saved', value: result.applicant.cells[index] ?? next };
    }
    if (result.kind === 'conflict') {
      bases.current[key] = result.latest;
      return { kind: 'conflict', latest: result.latest.cells[index] ?? '', by: 'シート', at: new Date().toISOString() };
    }
    return result;
  }, [saveRow]);
  const saver = useCellSaver(data.sheet.id, saveCell);
  return <>
    <Grid sheet={sheet} rows={rows} allRows={allRows} cols={cols.length ? cols : columns.slice(0, 1)} titleHeader={titleHeader}
      readOnly={data.stale} saver={saver} sort={sort} onSort={setSort} filters={filters}
      onFilter={(header, filter) => { setFilters((current) => filter ? { ...current, [header]: filter } : omit(current, header)); }}
      onOpen={onOpen} openRowNumber={selected} canEditColumns={false} onColumnSettings={() => undefined} jump={null} height={Math.max(300, Math.min(680, window.innerHeight - 230))}
      onEditStart={(row, column) => { const item = data.applicants.find((entry) => entry.row === row); if (item) bases.current[`${String(row)}|${column}`] = item; }} />
    {saver.toast && <ToastBar toast={saver.toast} onUndo={saver.undo} onClose={saver.closeToast} />}
  </>;
}
