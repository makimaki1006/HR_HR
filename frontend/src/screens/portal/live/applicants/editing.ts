import type { PortalApplicant } from '../../../../generated/PortalApplicant';
import type { PortalColumn } from '../../../../generated/PortalColumn';
import type { ColumnConfig, ColumnKind } from '../../types';

// 顧客APIで認可された列と行だけを編集する。
export type EditableColumn = PortalColumn;
export type EditableApplicant = PortalApplicant;
export interface RowChange { column: string; value: string }
export type SaveRowResult =
  | { kind: 'saved'; applicant: EditableApplicant }
  | { kind: 'conflict'; latest: EditableApplicant }
  | { kind: 'failed'; message: string };
export type SaveRow = (row: EditableApplicant, changes: RowChange[]) => Promise<SaveRowResult>;

export function columnConfig(column: EditableColumn): ColumnConfig {
  const kind: ColumnKind = ['choice', 'date', 'datetime', 'phone', 'email'].includes(column.kind) ? column.kind as ColumnKind : 'text';
  return {
    header: column.id,
    label: column.label,
    visible: true,
    writable: Boolean(column.id && column.writable && !(Array.isArray(column.options) && column.options.length === 0)),
    options: column.options ?? null,
    multiline: column.multiline,
    inList: column.in_list,
    kind,
    group: column.group === 'summary' || column.group === 'customer' || column.group === 'staff' ? column.group : 'other',
  };
}

export function validApplicant(value: unknown): value is EditableApplicant {
  if (value === null || typeof value !== 'object') return false;
  const item = value as Partial<EditableApplicant>;
  return Number.isInteger(item.row) && Array.isArray(item.cells) && item.cells.every((cell) => typeof cell === 'string') && typeof item.revision === 'string';
}
