// @vitest-environment happy-dom
// 条件タグの使われ方タブ: 帯別の表 3 枚と 2 つのグラフ。値は fixtures の JSON から決まる。
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { makeReport } from '../fixtures';
import { KeywordTab } from './KeywordTab';

afterEach(cleanup);

const must = <T,>(v: T | null | undefined): T => {
  if (v === null || v === undefined) throw new Error('expected a value');
  return v;
};
const cells = (row: HTMLElement): string[] => [...row.children].map((c) => c.textContent);

describe('KeywordTab', () => {
  it('帯別の表: 語・件数・全体の割合・人気求人での割合・差。n を添える', () => {
    render(<KeywordTab report={makeReport()} />);
    const heading = screen.getByRole('heading', { name: 'ほとんどの求人が付けている 1語（全体の50%以上）' });
    const rows = within(heading.nextElementSibling as HTMLElement).getAllByRole('row');
    expect(cells(must(rows[0]))).toEqual(['語', '件数', '全体の割合', '人気求人での割合', '差(pt)']);
    expect(cells(must(rows[1]))).toEqual(['交通費支給', '700', '56.7%', '75.0%', '+18.3']);
    expect(screen.getByText(/母数：全体 1234 件 \/ 人気求人 40 件/)).toBeTruthy();
    const half = screen.getByRole('heading', { name: '半数前後 1語（全体の20%以上50%未満）' });
    expect(cells(must(within(half.nextElementSibling as HTMLElement).getAllByRole('row')[1]))).toEqual([
      '昇給あり',
      '400',
      '32.4%',
      '20.0%',
      '-12.4',
    ]);
  });

  it('グラフ 2 つ: 帯別と人気求人との差', () => {
    render(<KeywordTab report={makeReport()} />);
    expect(screen.getByRole('img', { name: /キーワードを全体の割合で3つの帯に分けた語と割合/ })).toBeTruthy();
    expect(screen.getByRole('img', { name: /人気求人での割合から全体の割合を引いた差/ })).toBeTruthy();
  });

  it('SP 版でない CSV: 人気求人の列とグラフは出さず一文で伝える', () => {
    const r = makeReport();
    r.excel.tag_usage = { ...r.excel.tag_usage, popular_n: 0, popular_up: [], popular_down: [] };
    for (const b of r.excel.tag_usage.bands) {
      b.rows = b.rows.map((x) => ({ ...x, popular_tag_n: 0, popular_share_pct: null, gap_pt: null }));
    }
    render(<KeywordTab report={r} />);
    expect(screen.queryByRole('img', { name: /人気求人での割合から全体の割合を引いた差/ })).toBeNull();
    expect(screen.getAllByText('人気・超人気の表示があるSP版CSVのときだけ表示されます').length).toBeGreaterThan(0);
    expect(screen.queryByText('人気求人での割合')).toBeNull();
  });

  it('キーワードが無ければ空の文言', () => {
    const r = makeReport();
    for (const b of r.excel.tag_usage.bands) b.rows = [];
    render(<KeywordTab report={r} />);
    expect(screen.getAllByText('該当する語はありません').length).toBe(3);
    expect(screen.getByText('集計できるキーワードがありません')).toBeTruthy();
  });
});
