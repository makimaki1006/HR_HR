// @vitest-environment happy-dom
// 各 union 状態の描画。欠測 (null) は「—」、0 は 0。値は JSON から決まる。
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { GoogleDemand } from '../../generated/GoogleDemand';
import type { GoogleSection } from '../../generated/GoogleSection';
import type { GoogleSuggestions } from '../../generated/GoogleSuggestions';
import type { IndeedSection } from '../../generated/IndeedSection';
import type { PopulationSection } from '../../generated/PopulationSection';
import type { PopulationShares } from '../../generated/PopulationShares';
import { makeReport } from './fixtures';
import { ConsultationTab } from './tabs/ConsultationTab';
import { ExcelTab } from './tabs/ExcelTab';
import { GoogleTab } from './tabs/GoogleTab';
import { IndeedTab } from './tabs/IndeedTab';
import { PopulationTab } from './tabs/PopulationTab';

afterEach(cleanup);

const must = <T,>(v: T | null | undefined): T => {
  if (v === null || v === undefined) throw new Error('expected a value');
  return v;
};
/** セルの親の行 (tr)。 */
const rowOf = (cell: HTMLElement | undefined): HTMLElement => must(must(cell).parentElement);

const cells = (row: HTMLElement): string[] => [...row.children].map((c) => c.textContent);

const rowByLabel = (label: string): HTMLElement => {
  const th = screen.getAllByRole('rowheader', { name: label })[0];
  if (!th?.parentElement) throw new Error(`row not found: ${label}`);
  return th.parentElement;
};

describe('ExcelTab', () => {
  it('給与表: 小数 2 桁、null は —、0 は 0.00 (月給)', () => {
    render(<ExcelTab report={makeReport()} />);
    expect(cells(rowByLabel('平均値'))).toEqual(['平均値', '25.50', '32.12', '27.00', '—']);
  });

  it('給与表: 同点は Rust と同じ偶数丸め (32.125 → 32.12)', () => {
    render(<ExcelTab report={makeReport()} />);
    // 旧 format!("{:.2}", 32.125) は "32.12"
    expect(cells(rowByLabel('平均値'))[2]).toBe('32.12');
  });

  it('中央値の 0 は 0.00 で、— と区別される', () => {
    render(<ExcelTab report={makeReport()} />);
    expect(cells(rowByLabel('中央値'))).toEqual(['中央値', '24.00', '30.00', '0.00', '31.00']);
  });

  it('時給は小数 0 桁で単位は 円/時', () => {
    const r = makeReport();
    r.meta.unit = '円/時';
    r.meta.is_hourly = true;
    r.excel.decimals = 0;
    r.excel.salary_table = [{ label: '平均値', values: [1262.5, 1300, null, 0] }];
    render(<ExcelTab report={r} />);
    expect(screen.getByRole('heading', { name: '給与関係（円/時）' })).toBeTruthy();
    expect(cells(rowByLabel('平均値'))).toEqual(['平均値', '1262', '1300', '—', '0']);
    expect(screen.getByText(/給与分布は時給の実額/)).toBeTruthy();
  });

  it('月給は月給換算。分布の題に件数と刻み幅が出る', () => {
    render(<ExcelTab report={makeReport()} />);
    expect(screen.getByText(/給与分布は月給換算/)).toBeTruthy();
    // 題は図の見出し (figcaption) と SVG の title の 2 か所に出る
    expect(screen.getAllByText('上限ボリュームゾーン（万円・n=18・1刻み）').length).toBeGreaterThan(0);
    expect(screen.getAllByText('下限ボリュームゾーン（万円・n=6・1刻み）').length).toBeGreaterThan(0);
  });

  it('集計件数・差異表', () => {
    render(<ExcelTab report={makeReport()} />);
    expect(cells(rowByLabel('集計件数'))).toEqual(['集計件数', '1200', '1190', '20', '18']);
    const diff = screen.getByRole('heading', { name: '差異（総合 − 人気求人）' }).nextElementSibling;
    expect(diff?.tagName).toBe('TABLE');
    const rows = within(diff as HTMLElement).getAllByRole('row');
    expect(cells(must(rows[1]))).toEqual(['平均値', '-1.50', '—']);
    expect(cells(must(rows[2]))).toEqual(['中央値', '0.00', '-1.00']);
  });

  it('メタ表: 調査名・雇用形態・都道府県・市町村・件数 (桁区切り)', () => {
    render(<ExcelTab report={makeReport()} />);
    const text = document.body.textContent;
    expect(text).toContain('テスト調査');
    expect(text).toContain('正社員');
    expect(text).toContain('大阪市');
    expect(text).toContain('1,234');
  });

  it('メタが null のときは —', () => {
    const r = makeReport();
    r.meta.employment_type = null;
    r.meta.prefecture = null;
    r.meta.municipality = null;
    render(<ExcelTab report={r} />);
    const metaTable = must(screen.getAllByRole('table')[0]);
    expect(within(metaTable).getAllByText('—')).toHaveLength(3);
  });

  it('ワード表は上位 10 語だけ、占有率は小数 0 桁', () => {
    render(<ExcelTab report={makeReport()} />);
    const heading = screen.getByRole('heading', { name: '求人票ワード調査（全体）' });
    const table = heading.nextElementSibling as HTMLElement;
    const rows = within(table).getAllByRole('row');
    expect(rows).toHaveLength(1 + 10);
    expect(cells(must(rows[1]))).toEqual(['ワード1', '100', '1234', '8%']);
    expect(cells(must(rows[10]))[0]).toBe('ワード10');
  });

  it('先頭 N 件の比較表: 先頭率・全体率・差(pt)。全体が欠測の語は — (0 にしない)', () => {
    render(<ExcelTab report={makeReport()} />);
    const heading = screen.getByRole('heading', { name: '求人票ワード調査（先頭 45 件）' });
    const rows = within(heading.nextElementSibling as HTMLElement).getAllByRole('row');
    expect(cells(must(rows[0]))).toEqual(['上位10語', '件数', '先頭率', '全体率', '差(pt)']);
    expect(cells(must(rows[1]))).toEqual(['未経験歓迎', '30', '66.7%', '32.4%', '+34.3']);
    expect(cells(must(rows[2]))).toEqual(['賞与あり', '9', '20.0%', '—', '—']);
    expect(screen.getByText('母数：先頭 45 件 / 全体 1234 件。先頭は収録順、差は先頭率−全体率。')).toBeTruthy();
  });

  it('差(pt) は 0.05 未満なら符号なしの 0.0、負は -', () => {
    const r = makeReport();
    r.excel.keyword_comparison.rows = [
      { word: 'A', head_count: 1, head_share_pct: 50, all_count: 2, all_share_pct: 50.02 },
      { word: 'B', head_count: 1, head_share_pct: 20, all_count: 5, all_share_pct: 80 },
    ];
    render(<ExcelTab report={r} />);
    const heading = screen.getByRole('heading', { name: '求人票ワード調査（先頭 45 件）' });
    const rows = within(heading.nextElementSibling as HTMLElement).getAllByRole('row');
    expect(cells(must(rows[1]))[4]).toBe('0.0');
    expect(cells(must(rows[2]))[4]).toBe('-60.0');
  });

  it('比較の語が無いときは理由つきの空の文言', () => {
    const r = makeReport();
    r.excel.keyword_comparison = { head_n: 0, all_n: 10, rows: [] };
    const { unmount } = render(<ExcelTab report={r} />);
    expect(screen.getByText('取り込み順の比較データがありません')).toBeTruthy();
    unmount();
    r.excel.keyword_comparison = { head_n: 5, all_n: 10, rows: [] };
    render(<ExcelTab report={r} />);
    expect(screen.getByText('先頭の求人にキーワードがありません')).toBeTruthy();
  });

  it('全体のワードが空なら空の文言', () => {
    const r = makeReport();
    r.excel.keyword_all = [];
    render(<ExcelTab report={r} />);
    expect(screen.getByText('キーワードデータがありません')).toBeTruthy();
    expect(screen.getByText('集計できるキーワードがありません')).toBeTruthy();
  });

  it('グラフ 4 つ: 題に単位・件数・刻み、棒の数が JSON のビン数 (空の区間も棒)', () => {
    render(<ExcelTab report={makeReport()} />);
    const upper = screen.getByRole('img', { name: '上限ボリュームゾーン（万円・n=18・1刻み）' });
    expect(upper.querySelectorAll('rect')).toHaveLength(4);
    const lower = screen.getByRole('img', { name: '下限ボリュームゾーン（万円・n=6・1刻み）' });
    expect(lower.querySelectorAll('rect')).toHaveLength(2);
    expect(screen.getByRole('img', { name: '全体の上位20語を含む求人数' })).toBeTruthy();
    expect(
      screen.getByRole('img', {
        name: '先頭 45 件と全体 1234 件の占有率比較。先頭の上位20語。横軸0から100パーセント',
      }),
    ).toBeTruthy();
  });

  it('比較グラフ: 値は占有率 (%)。全体が欠測の語は全体の棒を描かず — を出す (0 にしない)', () => {
    const { container } = render(<ExcelTab report={makeReport()} />);
    const svg = must(container.querySelector('svg[data-series="keyword-head"]'));
    const bars = [...svg.querySelectorAll('rect[data-word]')].map((r) => [
      r.getAttribute('data-word'),
      r.getAttribute('data-group'),
      Number(r.getAttribute('data-value')),
    ]);
    expect(bars).toEqual([
      ['未経験歓迎', '全体', 32.4],
      ['未経験歓迎', '先頭', 66.666],
      ['賞与あり', '先頭', 20],
    ]);
    expect([...svg.querySelectorAll('text')].filter((t) => t.textContent === '—')).toHaveLength(1);
    expect(svg.querySelector('rect[data-word="賞与あり"][data-group="全体"]')).toBeNull();
    // 軸は 0〜100% の共通軸 (幅 440 は 5 目盛り)
    expect([...svg.querySelectorAll('text')].map((t) => t.textContent)).toEqual(
      expect.arrayContaining(['0%', '25%', '50%', '75%', '100%']),
    );
  });

  it('全体のグラフ: 棒の title は「語 / 全体: N件」で、上位 20 語まで', () => {
    const r = makeReport();
    r.excel.keyword_all = Array.from({ length: 25 }, (_, i) => ({
      word: `語${String(i)}`,
      count: 100 - i,
      jobs: 1234,
      share_pct: 1,
    }));
    const { container } = render(<ExcelTab report={r} />);
    const svg = must(container.querySelector('svg[data-series="keyword-all"]'));
    const titles = [...svg.querySelectorAll('rect > title')].map((t) => t.textContent);
    expect(titles).toHaveLength(20);
    expect(titles[0]).toBe('語0 / 全体: 100件');
    expect(titles[19]).toBe('語19 / 全体: 81件');
  });

  it('ヒストグラム: 最大 10 件で軸は 12、棒の高さは件数に比例し、0 件の棒は高さ 0。最多の帯の一文を出す', () => {
    render(<ExcelTab report={makeReport()} />);
    const upper = screen.getByRole('img', { name: '上限ボリュームゾーン（万円・n=18・1刻み）' });
    const heights = [...upper.querySelectorAll('rect')].map((r) => Number(r.getAttribute('height')));
    expect(heights[1]).toBeCloseTo((141 * 10) / 12, 6);
    expect(heights[0]).toBeCloseTo((141 * 3) / 12, 6);
    expect(heights[3]).toBe(0);
    expect(upper.getAttribute('viewBox')).toBe('0 0 900 200');
    expect(screen.getByText('最多の給与帯：21〜22 / 10件・55.6%')).toBeTruthy();
    // 最大が 1 本だけなので、その棒の上に件数が出る
    expect(within(upper).getByText('10件')).toBeTruthy();
  });

  it('ビンが空のグラフは「集計できるデータがありません」', () => {
    const r = makeReport();
    r.excel.histograms.lower = { bins: [], n: 0, step: 1, summary: null };
    render(<ExcelTab report={r} />);
    expect(screen.getByText('集計できるデータがありません')).toBeTruthy();
    expect(screen.queryByRole('img', { name: /^下限ボリュームゾーン/ })).toBeNull();
  });

  it('調査名の HTML は文字として出る (script 要素にならない)', () => {
    const r = makeReport();
    r.meta.title = '<script>alert(1)</script>';
    const { container } = render(<ExcelTab report={r} />);
    expect(container.querySelector('script')).toBeNull();
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});

const googleOk = (demand: GoogleDemand, suggestions: GoogleSuggestions, region = '大阪府'): GoogleSection => ({
  status: 'ok',
  keyword: 'テスト職 求人',
  region,
  demand,
  suggestions,
});
const DEMAND_OK: GoogleDemand = {
  status: 'ok',
  region_name: 'Osaka',
  keywords: [
    {
      keyword: 'テスト職 求人',
      avg_monthly: 1900,
      competition: 'HIGH',
      monthly_12m: [
        { month: '2025-10', search_volume: 1800 },
        { month: '2025-11', search_volume: 0 },
        { month: '2025-12', search_volume: null },
      ],
    },
    { keyword: '別の語', avg_monthly: null, competition: 'LOW', monthly_12m: [] },
  ],
};
const SUGG_OK: GoogleSuggestions = {
  status: 'ok',
  region_name: null,
  suggestions: [
    { keyword: '関連語A', avg_monthly: 500 },
    { keyword: '関連語B', avg_monthly: 0 },
    { keyword: '関連語C', avg_monthly: null },
  ],
};
const DEMAND_FAIL = 'Google検索需要を取得できませんでした。';

describe('GoogleTab', () => {
  it('not_requested / error は固定文だけ (表を出さない)', () => {
    for (const status of ['not_requested', 'error'] as const) {
      const { unmount } = render(
        <GoogleTab data={{ status, message: `メッセージ-${status}` }} />,
      );
      expect(screen.getByText(`メッセージ-${status}`)).toBeTruthy();
      expect(screen.queryAllByRole('table')).toHaveLength(0);
      unmount();
    }
  });

  it('出典の注記は常にある', () => {
    render(<GoogleTab data={{ status: 'not_requested', message: 'x' }} />);
    expect(screen.getByText(/Google広告 Keyword Planner API/)).toBeTruthy();
  });

  it('ok × ok: 検索需要表と関連語表。null は —、0 は 0', () => {
    render(<GoogleTab data={googleOk(DEMAND_OK, SUGG_OK)} />);
    expect(screen.getByText(/検索語: テスト職 求人 \/ 指定地域: 大阪府/)).toBeTruthy();
    expect(screen.getByText('取得地域: Osaka')).toBeTruthy();
    const demandRow = rowOf(screen.getAllByRole('cell', { name: 'テスト職 求人' })[0]);
    expect(cells(demandRow)).toEqual(['テスト職 求人', '1,900', 'HIGH']);
    const nullRow = rowOf(screen.getAllByRole('cell', { name: '別の語' })[0]);
    expect(cells(nullRow)).toEqual(['別の語', '—', 'LOW']);
    expect(cells(rowOf(screen.getByRole('cell', { name: '関連語B' })))).toEqual([
      '関連語B',
      '0',
    ]);
    expect(cells(rowOf(screen.getByRole('cell', { name: '関連語C' })))).toEqual([
      '関連語C',
      '—',
    ]);
  });

  it('月別検索数は開くまで行を出さない (DOM を増やさない)。開くと null は —、0 は 0', () => {
    render(<GoogleTab data={googleOk(DEMAND_OK, SUGG_OK)} />);
    expect(screen.queryByRole('cell', { name: '2025-10' })).toBeNull();
    const summary = screen.getByText('テスト職 求人 の月別検索数');
    fireEvent.click(summary);
    const row = (m: string): string[] => cells(rowOf(screen.getByRole('cell', { name: m })));
    expect(row('2025-10')).toEqual(['2025-10', '1,800']);
    expect(row('2025-11')).toEqual(['2025-11', '0']);
    expect(row('2025-12')).toEqual(['2025-12', '—']);
  });

  it('region_name が null で region 指定あり → 全国の注記', () => {
    render(<GoogleTab data={googleOk({ ...DEMAND_OK, region_name: null }, SUGG_OK)} />);
    expect(screen.getByText('指定地域を解決できなかったため全国の検索需要です。')).toBeTruthy();
  });

  it('region が空なら「全国」、取得地域の注記は無い', () => {
    render(<GoogleTab data={googleOk({ ...DEMAND_OK, region_name: null }, SUGG_OK, '')} />);
    expect(screen.getByText(/指定地域: 全国/)).toBeTruthy();
    expect(screen.queryByText(/解決できなかった/)).toBeNull();
  });

  it('キーワードが空なら空の文言', () => {
    render(<GoogleTab data={googleOk({ status: 'ok', region_name: 'x', keywords: [] }, SUGG_OK)} />);
    expect(screen.getByText('検索需要のデータがありません。')).toBeTruthy();
  });

  it.each(['missing_credentials', 'timeout', 'error'] as const)(
    '検索需要が %s のとき固定文 (生のエラーは出さない) で、関連語は別に出る',
    (status) => {
      render(<GoogleTab data={googleOk({ status }, SUGG_OK)} />);
      expect(screen.getByText(DEMAND_FAIL)).toBeTruthy();
      expect(screen.getByText('関連語A')).toBeTruthy();
    },
  );

  it.each(['missing_credentials', 'timeout', 'error'] as const)(
    '関連語が %s のとき空の見出しを出さず、検索需要は出る',
    (status) => {
      render(<GoogleTab data={googleOk(DEMAND_OK, { status })} />);
      expect(screen.queryByText(/関連キーワード/)).toBeNull();
      expect(screen.getAllByText('テスト職 求人').length).toBeGreaterThan(0);
      expect(screen.queryByText(DEMAND_FAIL)).toBeNull();
    },
  );

  it('両方失敗しても検索需要の固定文だけが出る (関連語は空の見出しなし)', () => {
    render(<GoogleTab data={googleOk({ status: 'timeout' }, { status: 'missing_credentials' })} />);
    expect(screen.getByText(DEMAND_FAIL)).toBeTruthy();
    expect(screen.queryByText(/関連キーワード/)).toBeNull();
  });

  it('語ごとの月間検索数の折れ線: 欠測の月は点を打たず、観測した 0 は点になる', () => {
    render(<GoogleTab data={googleOk(DEMAND_OK, SUGG_OK)} />);
    const svg = screen.getByRole('img', { name: 'テスト職 求人：月間検索数の推移' });
    const dots = [...svg.querySelectorAll('circle')].map((c) => [c.getAttribute('data-month'), c.getAttribute('data-value')]);
    expect(dots).toEqual([
      ['2025-10', '1800'],
      ['2025-11', '0'],
    ]);
    // 観測値の無い語 (別の語) はグラフを出さない
    expect(screen.queryByRole('img', { name: '別の語：月間検索数の推移' })).toBeNull();
  });

  it('関連語の取得地域は検索需要とは別。null は全国の注記、地域名があればその名前', () => {
    const { unmount } = render(<GoogleTab data={googleOk(DEMAND_OK, SUGG_OK)} />);
    expect(
      screen.getByText('関連キーワードの取得地域：全国（地域指定なし・地域未解決）。指定地域の需要とは限りません。'),
    ).toBeTruthy();
    unmount();
    render(<GoogleTab data={googleOk(DEMAND_OK, { ...SUGG_OK, region_name: 'Tokyo, Japan' })} />);
    expect(screen.getByText('関連キーワードの取得地域：Tokyo, Japan')).toBeTruthy();
  });

  it('関連語が空なら「需要0を意味しません」', () => {
    render(<GoogleTab data={googleOk(DEMAND_OK, { status: 'ok', region_name: null, suggestions: [] })} />);
    expect(screen.getByText('関連キーワードのデータがありません。需要0を意味しません。')).toBeTruthy();
  });
});

describe('IndeedTab', () => {
  it('unavailable は固定文のみ', () => {
    const data: IndeedSection = { status: 'unavailable', message: 'データがありません。' };
    render(<IndeedTab data={data} />);
    expect(screen.getByText('データがありません。')).toBeTruthy();
    expect(screen.queryAllByRole('table')).toHaveLength(0);
  });

  it('ok: 行の null は —、0 は 0、小数は 2 桁', () => {
    render(<IndeedTab data={makeReport().indeed} />);
    const r1 = rowOf(screen.getByRole('cell', { name: '2026-07' }));
    expect(cells(r1)).toEqual(['2026-07', '12,000', '345.50', '0', '1.25']);
    const r2 = rowOf(screen.getByRole('cell', { name: '2026-08' }));
    expect(cells(r2)).toEqual(['2026-08', '—', '—', '—', '—']);
  });

  it('見出し・出典・集計日・caveat', () => {
    render(<IndeedTab data={makeReport().indeed} />);
    expect(screen.getByText('テスト職 / 大阪府')).toBeTruthy();
    expect(document.body.textContent).toContain('出典：Indeed採用市場レポート｜全給与形態。閲覧人数は応募数ではありません。');
  });
});

describe('PopulationTab', () => {
  it('unavailable は集計地域と固定文', () => {
    const data: PopulationSection = {
      status: 'unavailable',
      region: '全国',
      message: '全国の人口データを取得できませんでした。',
    };
    render(<PopulationTab data={data} />);
    expect(screen.getByText('集計地域：全国')).toBeTruthy();
    expect(screen.getByText('全国の人口データを取得できませんでした。')).toBeTruthy();
    expect(screen.queryAllByRole('table')).toHaveLength(0);
  });

  it('構成比なし: 年齢別表 (男・女・合計、桁区切り) とピラミッド、基準日', () => {
    render(<PopulationTab data={makeReport().population} />);
    expect(screen.getByText('集計地域：大阪府')).toBeTruthy();
    expect(screen.getByText('人口の基準日：2020-10-01')).toBeTruthy();
    const row = rowOf(screen.getByRole('cell', { name: '5～9歳' }));
    expect(cells(row)).toEqual(['5～9歳', '110,000', '104,000', '214,000']);
    const zero = rowOf(screen.getByRole('cell', { name: '10～14歳' }));
    expect(cells(zero)).toEqual(['10～14歳', '120,000', '0', '120,000']);
    expect(screen.getByRole('img', { name: '人口ピラミッド' }).querySelectorAll('rect')).toHaveLength(6);
  });

  it('基準日が無ければ「未取得」', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(<PopulationTab data={{ ...d, reference_date: null }} />);
    expect(screen.getByText('人口の基準日：未取得')).toBeTruthy();
  });

  it('欠測 (null) の年齢帯: 表は — で残し、合計も — 。グラフは保留して 0 にしない', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(
      <PopulationTab
        data={{ ...d, bands: [{ age_group: '20-24', male: null, female: 100 }] }}
      />,
    );
    expect(cells(rowOf(screen.getByRole('cell', { name: '20-24' })))).toEqual(['20-24', '—', '100', '—']);
    expect(screen.getByText(/欠測のためグラフの表示を保留/)).toBeTruthy();
    expect(screen.queryByRole('img', { name: '人口ピラミッド' })).toBeNull();
  });

  it('観測した 0 は 0 のまま (欠測と区別)。グラフも出る', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(
      <PopulationTab data={{ ...d, bands: [{ age_group: '20-24', male: 0, female: 100 }] }} />,
    );
    expect(cells(rowOf(screen.getByRole('cell', { name: '20-24' })))).toEqual(['20-24', '0', '100', '100']);
    expect(screen.queryByText(/グラフの表示を保留/)).toBeNull();
    expect(screen.getByRole('img', { name: '人口ピラミッド' })).toBeTruthy();
  });

  it('bands が空: データなしの行と文言。最低賃金は出る', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(<PopulationTab data={{ ...d, bands: [] }} />);
    expect(screen.getByText('人口データがありません。')).toBeTruthy();
    expect(screen.getByText('データなし')).toBeTruthy();
    expect(screen.queryByRole('img', { name: '人口ピラミッド' })).toBeNull();
    expect(cells(rowByLabel('最低賃金（円/時）'))).toEqual(['最低賃金（円/時）', '1,064']);
  });

  it('最低賃金・労働統計の表: 日付はそのまま、null は —', () => {
    render(<PopulationTab data={makeReport().population} />);
    expect(cells(rowByLabel('最低賃金の改定年度'))[1]).toBe('2025');
    expect(cells(rowByLabel('最低賃金の発効日'))[1]).toBe('2025-10-16');
    expect(cells(rowByLabel('最低賃金の基準日（日本時間）'))[1]).toBe('2026-10-05');
    expect(cells(rowByLabel('最低賃金の出典'))[1]).toBe('厚生労働省の公式改定一覧');
    expect(cells(rowByLabel('労働統計の年度'))[1]).toBe('2024');
    expect(cells(rowByLabel('完全失業率（%）'))[1]).toBe('2.50');
    expect(cells(rowByLabel('離職率（%）'))[1]).toBe('—');
  });

  it('公式資料の URL があればリンクを出す (別タブ・noopener)', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(<PopulationTab data={{ ...d, minimum_wage_source_url: 'https://www.mhlw.go.jp/example' }} />);
    const link = screen.getByRole('link', { name: '公式資料を確認' });
    expect(link.getAttribute('href')).toBe('https://www.mhlw.go.jp/example');
    expect(link.getAttribute('rel')).toContain('noopener');
  });

  it('labor が null・賃金が null・出典の種別', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(
      <PopulationTab
        data={{ ...d, labor: null, minimum_wage: null, minimum_wage_fiscal_year: null, minimum_wage_source: 'database' }}
      />,
    );
    expect(cells(rowByLabel('最低賃金（円/時）'))[1]).toBe('—');
    expect(cells(rowByLabel('最低賃金の改定年度'))[1]).toBe('—');
    expect(cells(rowByLabel('最低賃金の出典'))[1]).toBe('外部統計データベース');
    expect(cells(rowByLabel('労働統計の年度'))[1]).toBe('—');
    expect(cells(rowByLabel('完全失業率（%）'))[1]).toBe('—');
  });

  it('未知の出典種別は —', () => {
    const d = makeReport().population;
    if (d.status !== 'ok') throw new Error('fixture');
    render(<PopulationTab data={{ ...d, minimum_wage_source: 'other' }} />);
    expect(cells(rowByLabel('最低賃金の出典'))[1]).toBe('—');
  });
});

/** 総人口 1000 (男 400・女 600)、年齢別は 20-29 歳だけ (男 100・女 200)。未収録分は男 300・女 400。 */
const SHARES: PopulationShares = {
  total: 1000,
  male: 400,
  female: 600,
  male_share_pct: 40,
  female_share_pct: 60,
  age_groups: [
    { label: '0〜14歳', count: 100, share_pct: 10 },
    { label: '15〜64歳', count: 100, share_pct: 10 },
    { label: '65歳以上', count: 100, share_pct: 10 },
    { label: '年齢区分未収録', count: 700, share_pct: 70 },
  ],
  bands: [
    { age_group: '20-29', male: 100, female: 200, male_share_pct: 10, female_share_pct: 20, total_share_pct: 30 },
  ],
  unrecorded: {
    age_group: '年齢区分未収録（総人口との差分）',
    male: 300,
    female: 400,
    male_share_pct: 30,
    female_share_pct: 40,
    total_share_pct: 70,
  },
  axis_pct: 20,
};

describe('PopulationTab (総人口を分母にした構成比・全国)', () => {
  const national = (shares: PopulationShares | null = SHARES): PopulationSection => ({
    status: 'ok',
    region: '全国',
    is_national: true,
    reference_date: '2020-10-01',
    shares,
    bands: [{ age_group: '20-29', male: 100, female: 200 }],
    minimum_wage: null,
    minimum_wage_fiscal_year: null,
    minimum_wage_effective_date: '',
    minimum_wage_as_of: '',
    minimum_wage_source: '',
    minimum_wage_source_url: null,
    labor: null,
  });

  it('全国は最低賃金・労働統計を出さない', () => {
    render(<PopulationTab data={national()} />);
    expect(screen.getByText('集計地域：全国')).toBeTruthy();
    expect(screen.queryByText('地域の最低賃金・労働統計')).toBeNull();
    expect(screen.queryAllByRole('rowheader', { name: '最低賃金（円/時）' })).toHaveLength(0);
  });

  it('総人口・男性・女性は総人口に対する割合', () => {
    render(<PopulationTab data={national()} />);
    const text = document.body.textContent;
    expect(text).toContain('総人口1,000人総人口の100.0%');
    expect(text).toContain('男性400人総人口の40.0%');
    expect(text).toContain('女性600人総人口の60.0%');
  });

  it('年齢 3 区分と未収録分で 100% になる帯・凡例', () => {
    render(<PopulationTab data={national()} />);
    const bar = screen.getByRole('img', { name: '年齢3区分と年齢区分未収録分の構成比' });
    const widths = [...bar.querySelectorAll('span')].map((s) => s.style.width);
    expect(widths).toEqual(['10.00000000%', '10.00000000%', '10.00000000%', '70.00000000%']);
    expect(document.body.textContent).toContain('年齢区分未収録 70.0%');
  });

  it('3 区分が成立しない (age_groups が null) ときは帯を出さない', () => {
    render(<PopulationTab data={national({ ...SHARES, age_groups: null })} />);
    expect(screen.queryByRole('img', { name: '年齢3区分と年齢区分未収録分の構成比' })).toBeNull();
    expect(screen.getByRole('img', { name: /人口ピラミッド。左：男性/ })).toBeTruthy();
  });

  it('ピラミッドの棒: 人数と総人口に対する割合 (男 100 人 = 10%、女 200 人 = 20%)。幅は軸 20% に対する比', () => {
    render(<PopulationTab data={national()} />);
    const svg = screen.getByRole('img', { name: /人口ピラミッド。左：男性/ });
    const rects = [...svg.querySelectorAll('rect')];
    expect(rects.map((r) => [r.getAttribute('data-sex'), r.getAttribute('data-count'), r.getAttribute('data-share')])).toEqual([
      ['male_count', '100', '10'],
      ['female_count', '200', '20'],
    ]);
    expect(Number(rects[0]?.getAttribute('width'))).toBeCloseTo(200, 8); // 10 / 20 * 400
    expect(Number(rects[1]?.getAttribute('width'))).toBeCloseTo(400, 8);
  });

  it('表: 年齢別と未収録分の行、合計は 100.00%', () => {
    render(<PopulationTab data={national()} />);
    expect(cells(rowOf(screen.getByRole('cell', { name: '20-29' })))).toEqual(['20-29', '100', '200', '300', '30.00%']);
    expect(
      cells(rowOf(screen.getByRole('cell', { name: '年齢区分未収録（総人口との差分）' }))),
    ).toEqual(['年齢区分未収録（総人口との差分）', '300', '400', '700', '70.00%']);
    expect(cells(rowByLabel('合計'))).toEqual(['合計', '400', '600', '1,000', '100.00%']);
  });

  it('構成比が無いとき (null) は割合を出さず人数の表だけ', () => {
    render(<PopulationTab data={national(null)} />);
    expect(screen.queryByText(/総人口の/)).toBeNull();
    expect(cells(rowOf(screen.getByRole('cell', { name: '20-29' })))).toEqual(['20-29', '100', '200', '300']);
  });
});

describe('IndeedTab 折れ線', () => {
  it('4 本の折れ線。欠測の月は点を打たず、観測した 0 は点になる', () => {
    render(<IndeedTab data={makeReport().indeed} />);
    const job = screen.getByRole('img', { name: '求人数の推移' });
    expect([...job.querySelectorAll('circle')].map((c) => c.getAttribute('data-month'))).toEqual(['2026-07']);
    const emp = screen.getByRole('img', { name: '募集企業数の推移' });
    const dots = [...emp.querySelectorAll('circle')];
    expect(dots).toHaveLength(1);
    expect(dots[0]?.getAttribute('data-value')).toBe('0');
    expect(screen.getByRole('img', { name: '1求人あたりに見た人数' })).toBeTruthy();
    expect(screen.getByRole('img', { name: '求人を見た人数の推移' })).toBeTruthy();
  });

  it('月別データが空なら折れ線は出さず、0 件ではない旨を表に出す', () => {
    const d = makeReport().indeed;
    if (d.status !== 'ok') throw new Error('fixture');
    render(<IndeedTab data={{ ...d, rows: [] }} />);
    expect(screen.queryByRole('img', { name: '求人数の推移' })).toBeNull();
    expect(screen.getByText('月別データがありません。0件を意味しません。')).toBeTruthy();
  });
});

describe('ConsultationTab', () => {
  const data = makeReport().consultation;

  it('給与: 中央値と有効件数、差。欠測は — (0 にしない)', () => {
    render(<ConsultationTab data={data} unit="万円/月" decimals={2} />);
    expect(screen.getByText('中央値（万円/月）')).toBeTruthy();
    expect(cells(rowByLabel('下限'))).toEqual(['下限', '24.00 / 1200件', '0.00 / 20件', '+24.00']);
    expect(cells(rowByLabel('上限'))).toEqual(['上限', '30.00 / 1190件', '— / 18件', '—']);
    expect(document.body.textContent).toContain('給与分布の有効件数：下限 1200 件・上限 1190 件。');
  });

  it('時給は小数 0 桁 (旧画面の円の整数表示)', () => {
    render(
      <ConsultationTab
        data={{
          ...data,
          salary: [{ label: '下限', all_median: 1200, all_n: 40, popular_median: 1250, popular_n: 12, delta: -50 }],
        }}
        unit="円/時"
        decimals={0}
      />,
    );
    expect(cells(rowByLabel('下限'))).toEqual(['下限', '1200 / 40件', '1250 / 12件', '-50']);
  });

  it('10 件未満の比較は参考値と注記', () => {
    const { unmount } = render(<ConsultationTab data={data} unit="万円/月" decimals={2} />);
    expect(screen.queryByText('10件未満の比較は参考値です。')).toBeNull();
    unmount();
    render(<ConsultationTab data={{ ...data, small_sample: true }} unit="万円/月" decimals={2} />);
    expect(screen.getByText('10件未満の比較は参考値です。')).toBeTruthy();
  });

  it('訴求の確認候補: 先頭 件/母数 と 全体 件/母数、差は負のポイント', () => {
    render(<ConsultationTab data={data} unit="万円/月" decimals={2} />);
    expect(cells(rowOf(screen.getByRole('cell', { name: '賞与あり' })))).toEqual([
      '賞与あり',
      '9/45 (20.0%)',
      '700/1234 (56.7%)',
      '-36.7',
    ]);
  });

  it('判断できない (insufficient) / 低い語なし (no_lower_share) はそれぞれの固定文', () => {
    const { unmount } = render(
      <ConsultationTab data={{ ...data, gaps: { status: 'insufficient' } }} unit="万円/月" decimals={2} />,
    );
    expect(screen.getByText(/差の判断を保留します/)).toBeTruthy();
    unmount();
    render(
      <ConsultationTab data={{ ...data, gaps: { status: 'no_lower_share' } }} unit="万円/月" decimals={2} />,
    );
    expect(screen.getByText(/訴求が十分であることの証明ではありません/)).toBeTruthy();
  });

  it('外部データの取得状況: 取得済み / 未取得', () => {
    render(
      <ConsultationTab
        data={{
          ...data,
          external: [
            { label: 'Google検索需要', fetched: true },
            { label: 'Indeed採用市場', fetched: false },
          ],
        }}
        unit="万円/月"
        decimals={2}
      />,
    );
    expect(cells(rowByLabel('Google検索需要'))).toEqual(['Google検索需要', '取得済み']);
    expect(cells(rowByLabel('Indeed採用市場'))).toEqual(['Indeed採用市場', '未取得']);
  });
});
