// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { makePopularitySplit, makeReport } from './fixtures';
import { ReportView } from './ReportView';
import { CompanyCountTab } from './tabs/CompanyCountTab';
import { PopularityTab, ratePct } from './tabs/PopularityTab';

afterEach(cleanup);

const rowText = (name: string): string[] => {
  const td = screen.getAllByText(name, { selector: 'td' })[0];
  const cells: Element[] = td?.parentElement ? Array.from(td.parentElement.children) : [];
  return cells.map((c) => c.textContent);
};

describe('PopularityTab', () => {
  it('割合は popular/n の四捨五入、n=0 は —', () => {
    expect(ratePct(19, 82)).toBe(23);
    expect(ratePct(30, 61)).toBe(49);
    expect(ratePct(0, 0)).toBeNull();
    render(<PopularityTab data={makePopularitySplit()} />);
    expect(rowText('0〜14日').slice(0, 4)).toEqual(['0〜14日', '82', '19', '23%']);
    expect(rowText('30日以上').slice(0, 4)).toEqual(['30日以上', '182', '57', '31%']);
    expect(rowText('不明').slice(0, 4)).toEqual(['不明', '0', '0', '—']);
  });

  it('掲載の長さ: 注記・キーワード・超人気ブロックが出る', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    expect(screen.getByText(/掲載日数は29日まで実日数/)).toBeTruthy();
    expect(screen.getByText('ブランクOK（n=12・人気 7）')).toBeTruthy();
    expect(screen.getByText('転勤なし（n=30・人気 14）')).toBeTruthy();
    const sec = within(screen.getByRole('region', { name: '14日以内の超人気' }));
    expect(sec.getByText('60.0%')).toBeTruthy();
    expect(sec.getByText('10.2%')).toBeTruthy();
  });

  it('対象の広さ: 帯の表・層別の —・30件未満の帯は事実だけ', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    fireEvent.click(screen.getByRole('button', { name: '対象の広さ' }));
    expect(rowText('2つ以上')).toEqual(['2つ以上', '80', '24.6%', '42', '52.5%', '39.6%']);
    expect(rowText('なし')[1]).toBe('200');
    expect(screen.getByText('求人数 n=325／人気・超人気 n=106')).toBeTruthy();
    expect(rowText('0〜3個')).toEqual(['0〜3個', '16.7%', '120', '—', '—']);
    expect(rowText('4〜6個')).toEqual(['4〜6個', '26.7%', '60', '58.0%', '50']);
    const g = within(screen.getByRole('region', { name: '間口を広げる語 なし' }));
    expect(g.getByText('長期休暇あり')).toBeTruthy();
    expect(g.getByText('+12.5')).toBeTruthy();
    expect(g.getByText('-8.3')).toBeTruthy();
    expect(screen.getByText('求人数 25 件のため、この帯の比較は出しません。')).toBeTruthy();
    expect(screen.getByText(/分け方: 求人に付く条件タグのうち、学歴不問・未経験者歓迎/)).toBeTruthy();
  });

  it('super_recent が無ければ超人気ブロックを出さない', () => {
    render(<PopularityTab data={{ ...makePopularitySplit(), super_recent: null }} />);
    expect(screen.queryByRole('region', { name: '14日以内の超人気' })).toBeNull();
  });

  it('サブタブ切り替え: 給与帯は中央値と件数だけ、欠測は —', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    fireEvent.click(screen.getByRole('button', { name: '給与帯' }));
    expect(rowText('人気・超人気')).toEqual(['人気・超人気', '106', '22.0万円']);
    expect(rowText('それ以外')).toEqual(['それ以外', '219', '—']);
    expect(rowText('〜20.5万').slice(0, 4)).toEqual(['〜20.5万', '67', '21', '31%']);
  });

  it('会社: 1件側が 0 のときの割合は —、勤務形態・わかりやすさ・書き方・条件タグ', () => {
    render(<PopularityTab data={makePopularitySplit()} />);
    fireEvent.click(screen.getByRole('button', { name: '会社' }));
    expect(rowText('2件以上')).toEqual(['2件以上', '48', '137', '36', '26%']);
    expect(rowText('1件')).toEqual(['1件', '0', '0', '0', '—']);
    expect(rowText('テスト工業')).toEqual(['テスト工業', '6', '5', '1.6', '+3.4', '会社に偏りがある']);
    expect(rowText('サンプル商事')).toEqual(['サンプル商事', '4', '1', '1.0', '0.0', '給与・条件の範囲']);
    expect(screen.getByText(/会社に偏りがある会社は 2 社/)).toBeTruthy();
    expect(screen.getByText(/全体の 8.5%/)).toBeTruthy();
    expect(rowText('学歴不問')).toEqual(['学歴不問', '67', '+16.3', '+15.1']);
    expect(rowText('寮完備')).toEqual(['寮完備', '8', '-4.2', '—']);
    fireEvent.click(screen.getByRole('button', { name: '条件タグ' }));
    expect(rowText('食事補助あり').slice(0, 4)).toEqual(['食事補助あり', '13', '8', '62%']);
    fireEvent.click(screen.getByRole('button', { name: '書き方' }));
    expect(rowText('26〜40字').slice(0, 4)).toEqual(['26〜40字', '105', '52', '50%']);
    fireEvent.click(screen.getByRole('button', { name: 'わかりやすさ' }));
    expect(screen.getByText(/1点ずつ数えた合計/)).toBeTruthy();
    expect(rowText('4点').slice(0, 4)).toEqual(['4点', '72', '35', '49%']);
    fireEvent.click(screen.getByRole('button', { name: '勤務形態' }));
    expect(rowText('夜勤').slice(0, 4)).toEqual(['夜勤', '17', '12', '71%']);
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
