// @vitest-environment happy-dom
// 各 union 状態の描画。欠測 (null) は「—」、0 は 0。値は JSON から決まる。
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { GoogleDemand } from '../../generated/GoogleDemand';
import type { GoogleSection } from '../../generated/GoogleSection';
import type { GoogleSuggestions } from '../../generated/GoogleSuggestions';
import type { IndeedSection } from '../../generated/IndeedSection';
import type { PopulationSection } from '../../generated/PopulationSection';
import { makeReport } from './fixtures';
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
    expect(screen.getByText(/給与分布は50円刻み/)).toBeTruthy();
  });

  it('月給は 1万円刻み', () => {
    render(<ExcelTab report={makeReport()} />);
    expect(screen.getByText(/給与分布は1万円刻み/)).toBeTruthy();
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

  it('上位 N 件の見出しは top_n_effective、占有率 66.666 → 67%', () => {
    render(<ExcelTab report={makeReport()} />);
    const heading = screen.getByRole('heading', { name: '求人票ワード調査（上位 45 件）' });
    const rows = within(heading.nextElementSibling as HTMLElement).getAllByRole('row');
    expect(cells(must(rows[1]))).toEqual(['未経験歓迎', '30', '45', '67%']);
  });

  it('ワードが空なら空の文言', () => {
    const r = makeReport();
    r.excel.keyword_head = [];
    render(<ExcelTab report={r} />);
    expect(screen.getByText('キーワードデータがありません')).toBeTruthy();
  });

  it('グラフ 4 つ: 見出しに単位、棒の数が JSON のビン数', () => {
    render(<ExcelTab report={makeReport()} />);
    const upper = screen.getByRole('img', { name: '上限ボリュームゾーン（万円）' });
    expect(upper.querySelectorAll('rect')).toHaveLength(4);
    const lower = screen.getByRole('img', { name: '下限ボリュームゾーン（万円）' });
    expect(lower.querySelectorAll('rect')).toHaveLength(2);
    expect(screen.getByRole('img', { name: '求人票キーワード調査（全体）' })).toBeTruthy();
    expect(screen.getByRole('img', { name: '求人票キーワード調査（上位 45 件）' })).toBeTruthy();
  });

  it('ヒストグラム: 最大の棒の高さが plot (155) で、0 件の棒は高さ 0', () => {
    render(<ExcelTab report={makeReport()} />);
    const upper = screen.getByRole('img', { name: '上限ボリュームゾーン（万円）' });
    const heights = [...upper.querySelectorAll('rect')].map((r) => Number(r.getAttribute('height')));
    expect(heights[1]).toBeCloseTo(155, 6);
    expect(heights[0]).toBeCloseTo(46.5, 6);
    expect(heights[3]).toBe(0);
    expect(upper.getAttribute('viewBox')).toBe('0 0 900 200');
  });

  it('ビンが空のグラフは「集計できるデータがありません」', () => {
    const r = makeReport();
    r.excel.histograms.lower = [];
    render(<ExcelTab report={r} />);
    expect(screen.getByText('集計できるデータがありません')).toBeTruthy();
    expect(screen.queryByRole('img', { name: '下限ボリュームゾーン（万円）' })).toBeNull();
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
  suggestions: [
    { keyword: '関連語A', avg_monthly: 500 },
    { keyword: '関連語B', avg_monthly: 0 },
    { keyword: '関連語C', avg_monthly: null },
  ],
};
const DEMAND_FAIL = 'Google検索需要を取得できませんでした。API設定または接続状況を確認してください。';
const SUGG_FAIL = '関連キーワードを取得できませんでした。';

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
    expect(screen.queryByText('2025-10')).toBeNull();
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
      expect(screen.queryByText(SUGG_FAIL)).toBeNull();
    },
  );

  it.each(['missing_credentials', 'timeout', 'error'] as const)(
    '関連語が %s のとき固定文で、検索需要は出る',
    (status) => {
      render(<GoogleTab data={googleOk(DEMAND_OK, { status })} />);
      expect(screen.getByText(SUGG_FAIL)).toBeTruthy();
      expect(screen.getAllByText('テスト職 求人').length).toBeGreaterThan(0);
      expect(screen.queryByText(DEMAND_FAIL)).toBeNull();
    },
  );

  it('両方失敗しても固定文が 2 つ出る', () => {
    render(<GoogleTab data={googleOk({ status: 'timeout' }, { status: 'missing_credentials' })} />);
    expect(screen.getByText(DEMAND_FAIL)).toBeTruthy();
    expect(screen.getByText(SUGG_FAIL)).toBeTruthy();
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
    expect(document.body.textContent).toContain('出典: テスト出典 / 集計日: 2026-09-30。テスト注意書き');
    expect(document.body.textContent).toContain('求人を見た人数は応募数ではありません');
  });
});

describe('PopulationTab', () => {
  it('unavailable は集計地域が空で固定文', () => {
    const data: PopulationSection = { status: 'unavailable', message: '都道府県を選択してください。' };
    render(<PopulationTab data={data} />);
    expect(screen.getByText('集計地域：')).toBeTruthy();
    expect(screen.getByText('都道府県を選択してください。')).toBeTruthy();
    expect(screen.queryAllByRole('table')).toHaveLength(0);
  });

  it('ok: 年齢別表 (男・女・合計、桁区切り) とピラミッド', () => {
    render(<PopulationTab data={makeReport().population} />);
    expect(screen.getByText('集計地域：大阪府')).toBeTruthy();
    const row = rowOf(screen.getByRole('cell', { name: '5～9歳' }));
    expect(cells(row)).toEqual(['5～9歳', '110,000', '104,000', '214,000']);
    const zero = rowOf(screen.getByRole('cell', { name: '10～14歳' }));
    expect(cells(zero)).toEqual(['10～14歳', '120,000', '0', '120,000']);
    expect(screen.getByRole('img', { name: '人口ピラミッド' }).querySelectorAll('rect')).toHaveLength(6);
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
