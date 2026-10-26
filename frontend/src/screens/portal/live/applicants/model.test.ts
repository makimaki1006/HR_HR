import { describe, expect, it } from 'vitest';
import type { PortalColumn } from '../../../../generated/PortalColumn';
import { listLayout, mailHref, matches, newestFirst, readUrl, telHref, urlFor } from './model';

const col = (label: string, kind: string, in_list: boolean, group = 'summary'): PortalColumn => ({ label, kind, group, in_list, multiline: false });

describe('listLayout', () => {
  it('一覧に出す項目から名前・状況・2 行目を決める', () => {
    const cols = [col('応募日', 'date', true), col('応募者名', 'text', true), col('状況', 'choice', true), col('職種', 'text', true), col('媒体', 'text', true), col('電話番号', 'phone', false)];
    expect(listLayout(cols)).toEqual({ list: [0, 1, 2, 3, 4], title: 1, status: 2, sub: [0, 3] });
  });
  it('一覧に出す項目が無ければ先頭の 1 つだけ', () => {
    expect(listLayout([col('応募者名', 'text', false), col('状況', 'choice', false)])).toEqual({ list: [0], title: 0, status: null, sub: [] });
  });
});

describe('matches', () => {
  const a = { row: 3, cells: ['山田 花子', '090-0000-0001', 'Sample1@Example.com'] };
  it('空白・大文字小文字・全角半角をそろえて探す', () => {
    expect(matches(a, '山田花子')).toBe(true);
    expect(matches(a, 'ＳＡＭＰＬＥ１')).toBe(true);
    expect(matches(a, '鈴木')).toBe(false);
  });
  it('電話番号はハイフンを除いても合う', () => {
    expect(matches(a, '09000000001')).toBe(true);
    expect(matches(a, '0000-0001')).toBe(true);
    expect(matches(a, '0002')).toBe(false);
  });
});

it('新しく入った行（下の行）を上にする', () => {
  expect(newestFirst([{ row: 3, cells: [] }, { row: 9, cells: [] }, { row: 5, cells: [] }]).map((r) => r.row)).toEqual([9, 5, 3]);
});

it('電話とメールのリンクは形が合うときだけ作る', () => {
  expect(telHref('090-0000-0001')).toBe('tel:09000000001');
  expect(telHref('不明')).toBeNull();
  expect(mailHref(' a@example.com ')).toBe('mailto:a@example.com');
  expect(mailHref('javascript:alert(1)')).toBeNull();
  expect(mailHref('a@b')).toBeNull();
});

it('URL のシートと行を読み書きする', () => {
  expect(readUrl('?sheet=s1&row=3')).toEqual({ sheet: 's1', row: 3 });
  expect(readUrl('?row=-1')).toEqual({ sheet: null, row: null });
  expect(urlFor('/portal', 's1', 3)).toBe('/portal?sheet=s1&row=3');
  expect(urlFor('/portal', null, null)).toBe('/portal');
});
