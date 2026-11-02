// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { makePopularitySplit, makeReport } from './fixtures';
import { ReportView } from './ReportView';
import { CompanyCountTab } from './tabs/CompanyCountTab';
import { PopularityTab } from './tabs/PopularityTab';
import { ratePct } from './featureModel';

afterEach(cleanup);

const figRow = (label: string): HTMLElement | null =>
  within(screen.getByRole('figure')).getByText(label).closest<HTMLElement>('.cmp-feat-row');

const rowText = (name: string, i = 0): string[] => {
  const td = screen.getAllByText(name, { selector: 'td' })[i];
  const cells: Element[] = td?.parentElement ? Array.from(td.parentElement.children) : [];
  return cells.map((c) => c.textContent);
};

describe('PopularityTab', () => {
  it('割合は popular/n の四捨五入、n=0 は —。上の帯は全体の件数と割合', () => {
    expect(ratePct(25, 34)).toBe(74);
    expect(ratePct(0, 0)).toBeNull();
    render(<PopularityTab data={makePopularitySplit()} />);
    expect(screen.getByText('全331件のうち、人気・超人気150件(45%)')).toBeTruthy();
    expect(screen.getAllByRole('button').slice(0, 6).map((b) => b.textContent.slice(0, 2))).toEqual(['給料', '待遇', '休み', '応募', '職場', '募集']);
  });

  it('休み・時間・通勤: 駅近5分以内 74% / 34件中25件、見方の文と軸', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    fireEvent.click(screen.getByRole('button', { name: /休み・時間・通勤/ }));
    expect(screen.getByRole('button', { name: /休み・時間・通勤/ }).getAttribute('aria-pressed')).toBe('true');
    const fig = screen.getByRole('figure');
    expect(fig.textContent).toContain('74%34件中25件');
    expect(fig.textContent).toContain('見方: 緑の棒は「');
    expect(fig.textContent).toContain('と載っている求人');
    expect(screen.getByText('載っていない求人')).toBeTruthy();
    expect(screen.getAllByText('元の表・全数値を見る').length).toBe(1);
  });

  it('給料: 4つのボタンで切り替え、帯は「この区分/ほかの区分」。月給以外は含めない注記', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    const names = ['下限の帯', '上限の帯', '月給の書き方', '上限と下限の差'];
    for (const n of names) expect(screen.getByRole('button', { name: n })).toBeTruthy();
    expect(screen.getByText('この区分の求人')).toBeTruthy();
    expect(screen.getByText('ほかの区分の求人')).toBeTruthy();
    expect(screen.getByText(/月給以外の求人\(ほか .*含めていない。/)).toBeTruthy();
    expect(figRow('25.0万円未満')?.textContent).toContain('38%52件中20件');
    fireEvent.click(screen.getByRole('button', { name: '上限と下限の差' }));
    expect(screen.getByRole('heading', { name: '上限と下限の差ごとの割合' })).toBeTruthy();
  });

  it('20件未満は薄色で(少なめ)、表は具体値', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    fireEvent.click(screen.getByRole('button', { name: /待遇・手当/ }));
    expect(screen.getByText('求人数 5件(少なめ)')).toBeTruthy();
    expect(figRow('昇格あり')?.querySelector<HTMLElement>('.cmp-feat-plot')?.style.opacity).toBe('0.45');
    expect(rowText('賞与あり').slice(0, 5)).toEqual(['賞与あり', '112', '64', '57%', '39%']);
  });

  it('features が空なら一文だけ', () => {
    render(<PopularityTab data={{ ...makePopularitySplit(), features: [] }} />);
    expect(screen.getByText('このCSVでは項目別の比較を出せませんでした。')).toBeTruthy();
    expect(screen.queryByRole('figure')).toBeNull();
  });
});

describe('CompanyCountTab', () => {
  it('最長の掲載は 30日以上 / N日', () => {
    render(<CompanyCountTab data={makePopularitySplit()} />);
    expect(rowText('テスト工業')).toEqual(['テスト工業', '6', '3', '2', '30日以上']);
    expect(rowText('サンプル商事')).toEqual(['サンプル商事', '4', '0', '0', '25日']);
    expect(screen.getByText(/求人タイトルは表示しません/)).toBeTruthy();
  });
});

describe('ReportView の新タブ', () => {
  it('popularity_split が無ければタブを出さず、?tab=popularity は給与・待遇に戻る', () => {
    render(<ReportView report={makeReport()} tab="popularity" onTabChange={() => undefined} />);
    expect(screen.queryByRole('tab', { name: '人気の分かれ方' })).toBeNull();
    expect(screen.getByRole('tab', { name: '給与・待遇' }).getAttribute('aria-selected')).toBe('true');
  });

  it('あれば人口・地域データの後ろに2タブ。キーボードで移動できる', () => {
    const seen: string[] = [];
    render(
      <ReportView
        report={{ ...makeReport(), popularity_split: makePopularitySplit() }}
        tab="population"
        onTabChange={(t) => {
          seen.push(t);
        }}
      />,
    );
    expect(screen.getAllByRole('tab').map((t) => t.textContent).slice(-2)).toEqual([
      '人気の分かれ方',
      '会社別の求人数',
    ]);
    fireEvent.keyDown(screen.getByRole('tab', { name: '人口・地域データ' }), { key: 'ArrowRight' });
    fireEvent.keyDown(screen.getByRole('tab', { name: '人口・地域データ' }), { key: 'End' });
    expect(seen).toEqual(['popularity', 'companies']);
  });
});
