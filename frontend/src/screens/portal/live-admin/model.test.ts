import { describe, expect, it } from 'vitest';
import type { PortalAdminColumn } from '../../../generated/PortalAdminColumn';
import { gidFromUrl, missingHeaders, moveColumn, parseRoute, routeHref, setColumn, withNewHeaders } from './model';

const col = (header: string, patch: Partial<PortalAdminColumn> = {}): PortalAdminColumn => ({
  header,
  label: '',
  visible: false,
  writable: false,
  in_list: false,
  multiline: false,
  kind: 'text',
  group: 'other',
  ...patch,
});

describe('画面の中の行き先', () => {
  it('?view= を読み、足りないときは顧客の一覧にする', () => {
    expect(parseRoute('')).toEqual({ view: 'customers' });
    expect(parseRoute('?view=customer&id=c-1')).toEqual({ view: 'customer', id: 'c-1' });
    expect(parseRoute('?view=customer')).toEqual({ view: 'customers' });
    expect(parseRoute('?view=sheet-add&customer=c-1')).toEqual({ view: 'sheet-add', customer: 'c-1' });
    expect(parseRoute('?view=sheet&id=s-1')).toEqual({ view: 'sheet', id: 's-1' });
    expect(parseRoute('?view=accounts')).toEqual({ view: 'accounts' });
    expect(parseRoute('?view=activity')).toEqual({ view: 'activity' });
    expect(parseRoute('?view=unknown')).toEqual({ view: 'customers' });
  });
  it('href と行き来できる', () => {
    for (const r of [{ view: 'customer', id: 'c 1&x' }, { view: 'sheet-add', customer: 'c-2' }, { view: 'activity' }] as const) {
      expect(parseRoute(routeHref(r))).toEqual(r);
    }
  });
});

describe('項目の設定', () => {
  it('書ける・一覧に出すにすると見せる。見せないにすると書けず一覧にも出さない', () => {
    let cols = [col('状況'), col('備考')];
    cols = setColumn(cols, 0, { writable: true });
    expect(cols[0]).toMatchObject({ visible: true, writable: true });
    cols = setColumn(cols, 1, { in_list: true });
    expect(cols[1]).toMatchObject({ visible: true, in_list: true });
    cols = setColumn(cols, 0, { visible: false });
    expect(cols[0]).toMatchObject({ visible: false, writable: false, in_list: false });
    expect(cols[1]).toMatchObject({ visible: true, in_list: true }, );
  });
  it('並びを入れ替え、端では動かさない', () => {
    const cols = [col('a'), col('b'), col('c')];
    expect(moveColumn(cols, 1, -1).map((c) => c.header)).toEqual(['b', 'a', 'c']);
    expect(moveColumn(cols, 2, 1)).toBe(cols);
    expect(moveColumn(cols, 0, -1)).toBe(cols);
  });
  it('読んだ見出しで新しい項目を見せないで足し、無くなった項目を知らせる', () => {
    const cols = [col('状況', { visible: true }), col('旧い列')];
    const next = withNewHeaders(cols, ['応募日', '状況', ' ', '応募日', '備考']);
    expect(next.map((c) => c.header)).toEqual(['状況', '旧い列', '応募日', '備考']);
    expect(next[2]).toMatchObject({ visible: false, writable: false });
    expect(withNewHeaders(cols, ['状況'])).toBe(cols);
    expect(missingHeaders(next, ['応募日', '状況', '備考'])).toEqual(['旧い列']);
  });
});

describe('URL のタブの番号', () => {
  it('#gid= と ?gid= を読む', () => {
    expect(gidFromUrl('https://docs.google.com/spreadsheets/d/abc/edit#gid=123')).toBe(123);
    expect(gidFromUrl('https://docs.google.com/spreadsheets/d/abc/edit?gid=0')).toBe(0);
    expect(gidFromUrl('https://docs.google.com/spreadsheets/d/abc/edit')).toBeNull();
  });
});
