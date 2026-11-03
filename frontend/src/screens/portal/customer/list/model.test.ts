import { setPortalPreviewClock } from '../../clock';
import { NOW } from '../../now';
import { beforeEach, describe, expect, it } from 'vitest';
import { formatDayJa } from '../../columns';
import { getData } from '../../data';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { columnWidths, matchesQuery, norm, rowRefDay, sheetBaseRef } from './model';
import { suggestedStatuses } from './StatusSheet';

beforeEach(() => { setPortalPreviewClock(NOW); });

const sheetOf = (id: string): SheetConfig => {
  const s = getData().sheets.find((x) => x.id === id);
  if (!s) throw new Error(`no sheet ${id}`);
  return s;
};

describe('norm（検索の寄せ）', () => {
  it('ひらがな・カタカナ・半角カナを同じにする', () => {
    expect(norm('たなか')).toBe(norm('タナカ'));
    expect(norm('ﾀﾅｶ')).toBe(norm('たなか'));
    expect(norm('０９０−１２３４')).toBe('0901234');
  });
  it('一覧の検索で「たなか」「タナカ」が同じ件数になる', () => {
    const s1 = sheetOf('s1');
    const rows = getData().rows.s1 ?? [];
    const cols: ColumnConfig[] = s1.columns.filter((c) => c.header === 'フリガナ' || c.header === '応募者名');
    const hits = (q: string): number => rows.filter((r) => matchesQuery(r, cols, q)).length;
    expect(hits('たなか')).toBeGreaterThan(0);
    expect(hits('たなか')).toBe(hits('タナカ'));
  });
});

describe('suggestedStatuses（次によく使う）', () => {
  it('面接日時確定の次は 面接済・内定・入社日調整中', () => {
    const s1 = sheetOf('s1');
    const opts = s1.columns.find((c) => c.header === s1.statusColumn)?.options ?? [];
    expect(suggestedStatuses(s1, opts, '面接日時確定')).toEqual(['面接済', '内定', '入社日調整中']);
  });
});

describe('シートの基準の年', () => {
  it('2025年度のシートでは年の無い応募日を 2025 年として出す', () => {
    const s2 = sheetOf('s2');
    expect(sheetBaseRef(s2, getData().rows.s2 ?? [])).not.toBeNull();
    const rows = getData().rows.s2 ?? [];
    const row = rows.find((r) => /^\d{1,2}月\d{1,2}日$/.test(r.values[s2.sortColumn ?? ''] ?? ''));
    if (!row) return;
    const text = formatDayJa(row.values[s2.sortColumn ?? ''] ?? '', rowRefDay(s2, row, rows))?.text ?? '';
    expect(text.startsWith('2025/')).toBe(true);
  });
  it('今年のシートは null', () => {
    expect(sheetBaseRef(sheetOf('s1'), getData().rows.s1 ?? [])).toBeNull();
  });
});

describe('columnWidths（日付の列の幅）', () => {
  it('年付きの表示「2025/10/19（日）」が入る幅にする', () => {
    const col: ColumnConfig = { header: '面接予定日', label: '', visible: true, writable: true, options: null, multiline: false, inList: true, kind: 'date' };
    const rows: ApplicantRow[] = [{ rowNumber: 2, values: { 面接予定日: '2025/10/19' }, changedSinceLastView: null }];
    const w = columnWidths([col], rows, '応募者名', undefined)['面接予定日'] ?? 0;
    expect(w).toBeGreaterThanOrEqual(150);
  });
});
