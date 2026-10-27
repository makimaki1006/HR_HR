import { expect, Page, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import * as path from 'node:path';
import { COMPETITOR_FIXTURE as K } from './helpers/fixture_values';
import { login } from './helpers/login';

/**
 * 競合調査: 旧画面 (/competitor → /report/competitor の HTML) と新画面 (/app/competitor) の値一致 E2E。
 *
 * 同じ fixture CSV (tests/fixtures/competitor/sp_utf8.csv、架空の合成データ) と同じ条件を両方に送り、
 * 画面に出た値 (表のセル・SVG の階級と件数・状態の文言) を取り出して比べる。要素の存在では判定しない。
 * タブは 4 つ (給与・待遇 / Google / Indeed / 人口)。採用のヒントは出さない。
 * 判定は 旧 == 新 == 既知値 の 3 点。既知値 (helpers/fixture_values.ts の COMPETITOR_FIXTURE) は
 * scripts/e2e/competitor_expected.py が CSV から Rust と無関係に計算した値で、Rust の出力のコピーではない。
 *
 * 外部 API は呼ばない: global-setup.ts が GOOGLE_* 等を子プロセスの環境から除くので、Google は常に
 * 資格情報なし (missing_credentials) の経路。Indeed 採用市場 DB は E2E の環境に無い (unavailable)。
 * Google の ok / error / timeout の表示は Rust の契約テストと React の単体テストで確かめている。
 */

const CSV = path.resolve(__dirname, '../../fixtures/competitor/sp_utf8.csv');
const JOB_NAME = '施設長';
const TOP_N = '10';

interface Conditions {
  /** 職種名 (報告書の題名は 都道府県+市区町村 + 職種名 から組み立てる)。 */
  jobName: string;
  prefecture: string;
  wageMode: 'monthly' | 'hourly';
  google: boolean;
  keyword: string;
}

const CASE_MONTHLY: Conditions = {
  jobName: JOB_NAME, prefecture: '大阪府', wageMode: 'monthly', google: true, keyword: '施設長 求人',
};
const CASE_HOURLY: Conditions = {
  jobName: '', prefecture: '', wageMode: 'hourly', google: false, keyword: '',
};

// ---------------------------------------------------------------- 画面から値を取り出す (旧・新で同じ関数)

interface Extracted {
  meta: string[][];
  headings: string[];
  salary: string[][];
  diff: string[][];
  /** 全体の割合の帯別の表 (件数が無い帯は出ないので枚数は可変)。見出し (h3) と表を同じ順で持つ。 */
  bandTitles: string[];
  bands: string[][][];
  wordsHead: string[][];
  asideNotes: string[];
  caption: string[];
  charts: { caption: string; notes: string[]; bars: string[]; axis: string[]; geometry: string[] }[];
  google: { paragraphs: string[]; tables: string[][][] };
  indeed: { paragraphs: string[]; tables: string[][][] };
  population: { paragraphs: string[]; tables: string[][][] };
}

/**
 * 旧 (サーバ HTML) と新 (React) で共通の DOM 構造だけを使って取り出す。
 * 関数形式の evaluate なので CSP の eval には当たらない。非表示のタブパネルも textContent で読む。
 */
async function extract(page: Page): Promise<Extracted> {
  return page.evaluate(() => {
    const norm = (s: string | null | undefined): string => (s ?? '').replace(/\s+/g, ' ').trim();
    const rowsOf = (t: Element): string[][] =>
      Array.from((t as HTMLTableElement).rows).map((r) => Array.from(r.cells).map((c) => norm(c.textContent)));
    const tablesOf = (root: ParentNode): string[][][] => Array.from(root.querySelectorAll('table')).map(rowsOf);
    // 新画面のページ見出しの副題は <p class="cmp-sub"> (旧は別の要素) なので、段落の比較からは除く。文言は見出しとして別に確認する
    const parasOf = (root: ParentNode): string[] => Array.from(root.querySelectorAll('p:not(.cmp-sub)')).map((p) => norm(p.textContent));
    const panel = (id: string): HTMLElement => {
      const el = document.getElementById(`panel-${id}`);
      if (!el) throw new Error(`#panel-${id} が無い`);
      return el;
    };
    const excel = panel('excel');
    const aside = excel.querySelector('aside');
    if (!aside) throw new Error('aside が無い');
    const tables = Array.from(aside.querySelectorAll('table'));
    // 固定の 3 枚 (メタ・給与・差異) + 帯別の表 (0〜3 枚) + 先頭 N 件の比較表 (最後の 1 枚)
    if (tables.length < 4 || tables.length > 7) throw new Error(`集計表が ${String(tables.length)} 枚 (期待 4〜7)`);
    const bandTables = tables.slice(3, -1);
    const bandTitles = Array.from(aside.querySelectorAll('h3')).map((h) => norm(h.textContent));
    if (bandTitles.length !== bandTables.length) {
      throw new Error(`帯の見出し ${String(bandTitles.length)} 件と表 ${String(bandTables.length)} 枚が合わない`);
    }
    const r1 = (n: number): string => (Math.round(n * 10) / 10).toFixed(1);
    const charts = Array.from(excel.querySelectorAll('figure')).map((f) => ({
      caption: norm(f.querySelector('figcaption')?.textContent),
      notes: Array.from(f.querySelectorAll('p')).map((p) => norm(p.textContent)),
      bars: Array.from(f.querySelectorAll('rect > title')).map((t) => norm(t.textContent)),
      // 帯別・人気求人との差のグラフ (viewBox 固定、旧 = サーバ SVG、新 = React) は文字が g の入れ子の深さで違うので、全 text を順に比べる
      axis: f.classList.contains('tag-usage') || f.className.includes('tag-usage')
        ? Array.from(f.querySelectorAll('svg text')).map((t) => norm(t.textContent))
        : Array.from(f.querySelectorAll('svg > text, svg > g > text')).map((t) => norm(t.textContent)),
      // 棒の位置と高さ (小数 1 桁): 件数が同じでも棒の長さが旧と違えば検知する (旧は小数 3 桁、新は倍精度なので 1 桁に丸めて比べる)
      geometry: Array.from(f.querySelectorAll('rect')).map(
        (r) => `${r1(Number(r.getAttribute('x')))}/${r1(Number(r.getAttribute('width')))}/${r1(Number(r.getAttribute('height')))}`,
      ),
    }));
    return {
      meta: rowsOf(tables[0]!),
      headings: Array.from(aside.querySelectorAll('h2'))
        .map((h) => norm(h.textContent))
        .filter((t) => t !== '競合調査'),
      salary: rowsOf(tables[1]!),
      diff: rowsOf(tables[2]!),
      bandTitles,
      bands: bandTables.map(rowsOf),
      wordsHead: rowsOf(tables[tables.length - 1]!),
      asideNotes: Array.from(aside.querySelectorAll(':scope > p')).map((p) => norm(p.textContent)),
      caption: Array.from(excel.querySelectorAll(':scope > p')).map((p) => norm(p.textContent)),
      charts,
      google: { paragraphs: parasOf(panel('google')), tables: tablesOf(panel('google')) },
      indeed: { paragraphs: parasOf(panel('indeed')), tables: tablesOf(panel('indeed')) },
      population: { paragraphs: parasOf(panel('population')), tables: tablesOf(panel('population')) },
    };
  });
}

// ---------------------------------------------------------------- 旧・新の操作

async function fillCommon(page: Page, c: Conditions, legacy: boolean): Promise<void> {
  if (legacy) {
    // 旧画面のフォームには職種名の欄が無いので、同じ名前の入力を足して同じ値を送る (サーバの題名の組み立てを両方で確かめる)
    await page.evaluate((v) => {
      const i = document.createElement('input');
      i.type = 'hidden';
      i.name = 'job_name';
      i.value = v;
      document.getElementById('competitor-form')!.appendChild(i);
    }, c.jobName);
  } else {
    await page.locator('#job-name').fill(c.jobName);
  }
  await page.locator('#prefecture').selectOption(c.prefecture === '' ? { value: '' } : { label: c.prefecture });
  await page.locator('#wage-mode').selectOption(c.wageMode);
  await page.locator('#top-n').fill(TOP_N);
  await page.locator('#search-keyword').fill(c.keyword);
  const g = page.locator('input[name="include_google"]');
  if (c.google) await g.check();
  else await g.uncheck();
  await page.locator('#csv-file').setInputFiles(CSV);
}

async function legacyReport(page: Page, c: Conditions): Promise<Extracted> {
  await page.goto('/competitor');
  await fillCommon(page, c, true);
  await page.getByRole('button', { name: '画面で確認' }).click();
  await expect(page).toHaveURL(/\/report\/competitor$/);
  await expect(page.locator('#panel-excel')).toBeVisible();
  return extract(page);
}

async function appReport(page: Page, c: Conditions): Promise<Extracted> {
  await page.goto('/app/competitor');
  await expect(page.locator('#csv-file')).toBeVisible();
  await fillCommon(page, c, false);
  await page.getByRole('button', { name: 'レポートを作成' }).click();
  await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
  await expect(page.locator('#panel-excel')).toBeVisible();
  return extract(page);
}

// ---------------------------------------------------------------- 既知値 (fixture から別途確定した値)

/** 報告書の題名: 「{都道府県}{市区町村} {職種名}」。どちらも無ければ既定名。 */
function expectedTitle(c: Conditions): string {
  if (c.prefecture === '' && c.jobName === '') return 'Indeed競合調査';
  return c.prefecture !== '' && c.jobName !== '' ? `${c.prefecture} ${c.jobName}` : `${c.prefecture}${c.jobName}`;
}

const SALARY_LABELS = ['平均値', '中央値', '最頻値'];

function knownSalary(mode: 'monthly' | 'hourly'): string[][] {
  const m = K[mode];
  return [
    ['', '総合', '人気求人'], // 見出し行は colspan=2 ずつ
    ['', '下限', '上限', '下限', '上限'],
    ...SALARY_LABELS.map((label, i) => [
      label, m.all_lower[i]!, m.all_upper[i]!, m.pop_lower[i]!, m.pop_upper[i]!,
    ]),
    ['集計件数', ...m.counts.map(String)],
  ];
}

function knownDiff(mode: 'monthly' | 'hourly'): string[][] {
  const m = K[mode];
  return [['', '下限', '上限'], ...SALARY_LABELS.map((l, i) => [l, m.diff_lower[i]!, m.diff_upper[i]!])];
}

/**
 * 全体の割合の帯別 (求人票キーワード調査)。fixture の語は 6 語で、全体の割合は 26.7〜30.0% なので全部「半数前後」
 * (20% 以上 50% 未満)。50% 以上と、20% 未満で 3 件以上の帯は 0 語で「該当する語はありません」になる。
 * 語の順は 人気求人での割合 − 全体の割合 の大きい順。割合は N/60・M/18 (M = 人気・超人気の件数) を JS で
 * 小数 1 桁にした値、差は小数 1 桁に丸める前の値の差。件数 N は K.keywords_all、M は K.comparison から取る。
 */
interface Band { title: string; rows: string[][] }

function knownBands(): Band[] {
  const header = ['語', '件数', '全体の割合', '人気求人での割合', '差(pt)'];
  const empty = [[ '該当する語はありません' ]];
  const words = K.keywords_all.map(([word, n]) => {
    const pop = K.comparison.find((c) => c.word === word)!.popular_any;
    const all = (n / K.total) * 100;
    const popular = (pop / K.popular_any_n) * 100;
    const d = popular - all;
    return { word, n, pop, all, popular, d };
  });
  words.sort((x, y) => y.d - x.d);
  const mid = words.filter((w) => w.all >= 20 && w.all < 50);
  expect(mid.length, '全 6 語が半数前後の帯に入る fixture').toBe(words.length);
  return [
    { title: 'ほとんどの求人が付けている 0語（全体の50%以上）', rows: [header, ...empty] },
    {
      title: `半数前後 ${String(mid.length)}語（全体の20%以上50%未満）`,
      rows: [
        header,
        ...mid.map((w) => [
          w.word, String(w.n), `${w.all.toFixed(1)}%`, `${w.popular.toFixed(1)}%`,
          `${w.d >= 0 ? '+' : ''}${w.d.toFixed(1)}`,
        ]),
      ],
    },
    { title: '一部の求人だけ 0語（全体の20%未満（3件以上））', rows: [header, ...empty] },
  ];
}

/** 帯別グラフ・差のグラフの棒の title: 「語: N件・全体% / 人気求人 M件・人気% / 差 d」。 */
function knownBandBarTitle(row: string[], pop: number): string {
  return `${row[0]!}: ${row[1]!}件・${row[2]!} / 人気求人 ${String(pop)}件・${row[3]!} / 差 ${row[4]!}`;
}

/** 先頭 N 件と全体の比較表 (上位 10 語)。全体に語が無いときは —。 */
function knownComparison(): string[][] {
  return [
    ['上位10語', '件数', '先頭率', '全体率', '差(pt)', '人気・超人気率'],
    ...K.comparison.slice(0, 10).map((c) => [
      c.word,
      String(c.head),
      `${c.head_share}%`,
      c.all_share === null ? '—' : `${c.all_share}%`,
      c.delta ?? '—',
      c.popular_any_share === null ? '—' : `${c.popular_any_share}%`,
    ]),
  ];
}

function knownBars(bins: readonly (readonly [string, number])[]): string[] {
  return bins.map(([label, n]) => `${label}: ${String(n)}件`);
}

function jstToday(): string {
  return new Date(Date.now() + 9 * 3600_000).toISOString().slice(0, 10);
}

/** 3 者一致の本体: 旧 == 既知値、新 == 既知値 (どちらかが外れたら、どちらの画面かが分かる)。 */
function assertKnown(label: string, x: Extracted, c: Conditions): void {
  const mode = c.wageMode;
  const unit = mode === 'monthly' ? '万円' : '円/時';
  const tag = (s: string): string => `${label}: ${s}`;
  expect(x.meta, tag('概要表')).toEqual([
    ['調査名', '雇用形態'],
    [expectedTitle(c), '正社員'],
    ['該当都道府県', '主な市町村'],
    // 都道府県を選んだときは選んだ地域を使い (市区町村は未選択なので —)、選ばないときは CSV から読み取った値
    ['大阪府', c.prefecture === '' ? '大阪市' : '—'],
    ['集計対象', '該当件数'],
    ['CSV重複排除後', String(K.total)],
  ]);
  expect(x.headings, tag('見出し')).toEqual([
    `給与関係（${unit}）`,
    '差異（総合 − 人気求人）',
    '求人票キーワード調査（帯別）',
    `求人票ワード調査（先頭 ${String(K.head_n)} 件）`,
  ]);
  expect(x.salary, tag('給与表')).toEqual(knownSalary(mode));
  expect(x.diff, tag('差異表')).toEqual(knownDiff(mode));
  const bands = knownBands();
  expect(x.bandTitles, tag('帯の見出し')).toEqual(bands.map((b) => b.title));
  expect(x.bands, tag('帯別の表')).toEqual(bands.map((b) => b.rows));
  expect(x.wordsHead, tag('先頭 10 件と全体の比較表')).toEqual(knownComparison());
  expect(x.asideNotes, tag('比較の母数と注記')).toEqual([
    `母数：全体 ${String(K.total)} 件 / 人気求人 ${String(K.popular_any_n)} 件（人気または超人気のタグがある求人）。件数1〜2の語（0語）は帯に入れていません。差は人気求人での割合−全体の割合。`,
    `母数：先頭 ${String(K.head_n)} 件 / 全体 ${String(K.total)} 件。先頭は収録順、差は先頭率−全体率。`,
    `人気・超人気 ${String(K.popular_any_n)} 件（人気または超人気のタグがある求人）の中での占有率。`,
    '人気求人：Indeedの「人気」「超人気」タグ付き。',
  ]);
  expect(x.caption, tag('キャプション')).toEqual([
    `CSV重複排除後 ${String(K.total)} 件 / 上位 ${String(K.head_n)} 件は収録順。占有率は語を含む求人数の割合。給与分布は${mode === 'monthly' ? '月給換算' : '時給の実額'}。`,
  ]);

  const [upper, lower, bandChart, diffChart] = x.charts;
  const histN = K[mode].hist_n;
  const step = K[mode].hist_step;
  expect(x.charts.map((ch) => ch.caption), tag('グラフの題')).toEqual([
    `上限ボリュームゾーン（${unit}・n=${String(histN)}・${step}刻み）`,
    `下限ボリュームゾーン（${unit}・n=${String(histN)}・${step}刻み）`,
    '求人票キーワード調査（全体の割合の帯別）',
    '人気求人に多い語・少ない語',
  ]);
  expect(upper!.bars, tag('上限ヒストグラム')).toEqual(knownBars(K[mode].hist_upper));
  expect(lower!.bars, tag('下限ヒストグラム')).toEqual(knownBars(K[mode].hist_lower));
  // 帯別グラフ: 半数前後の 6 語の棒 (「該当する語はありません」の帯に棒は無い)。title は表の行と同じ値
  const midRows = bands[1]!.rows.slice(1);
  const popOf = (word: string): number => K.comparison.find((c) => c.word === word)!.popular_any;
  expect(bandChart!.bars, tag('帯別グラフの棒')).toEqual(midRows.map((r) => knownBandBarTitle(r, popOf(r[0]!))));
  expect(bandChart!.notes, tag('帯別グラフの注記')).toEqual([
    `全体 ${String(K.total)} 件のうち、その語が付く求人の割合。件数1〜2の語（0語）は帯に入れていません。`,
  ]);
  expect(bandChart!.axis, tag('帯別グラフの軸と見出し')).toEqual(
    expect.arrayContaining(['0%', '50%', '100%', bands[0]!.title, '該当する語はありません', bands[1]!.title, bands[2]!.title]),
  );
  // 人気求人との差のグラフ: 差の大きい上位 3 語 (多い) と、小さい下位 3 語 (少ない、最も小さい語から)。fixture は 6 語なので 3 + 3
  const more = midRows.slice(0, 3);
  const fewer = midRows.slice(3).reverse();
  expect(diffChart!.bars, tag('差のグラフの棒')).toEqual([...more, ...fewer].map((r) => knownBandBarTitle(r, popOf(r[0]!))));
  expect(diffChart!.axis, tag('差のグラフの見出しと軸')).toEqual(
    expect.arrayContaining(['-30', '0', '+30', '人気求人に多い語 3語', '人気求人に少ない語 3語']),
  );
  expect(diffChart!.notes, tag('差のグラフの注記')).toEqual([
    `棒は人気求人での割合から全体の割合を引いた差（ポイント）。人気求人 ${String(K.popular_any_n)} 件、全体 ${String(K.total)} 件。件数3以上の語が対象。`,
  ]);
  // 上位 10 語の比較表は先頭 N 件と全体の比較 (表のみ。グラフは作らない)
  // 空の給与区間も 0 件の棒として残る (階級が連続): 件数 0 の棒が 1 つ以上ある
  if (mode === 'monthly') expect(K[mode].hist_upper.some(([, n]) => n === 0), tag('空の給与区間を残す')).toBe(true);
  // ヒストグラムの件数の合計は、月給モードは換算後の全件 (60)、時給モードは時給の行 (16)
  const total = (bins: readonly (readonly [string, number])[]): number => bins.reduce((a, [, n]) => a + n, 0);
  expect(total(K[mode].hist_upper), tag('上限ヒストグラムの合計')).toBe(mode === 'monthly' ? K.total : K.hourly.counts[0]);
  expect(total(K[mode].hist_lower), tag('下限ヒストグラムの合計')).toBe(mode === 'monthly' ? K.total : K.hourly.counts[0]);

  // Google / Indeed / 人口の状態 (CI は外部 API 未設定。Indeed は職種を選ばないので「データがありません」の文言)
  if (c.google) {
    expect(x.google.paragraphs, tag('Google')).toContain(`検索語: ${c.keyword} / 指定地域: ${c.prefecture}`);
    expect(x.google.paragraphs, tag('Google 需要の失敗文')).toContain(
      'Google検索需要を取得できませんでした。',
    );
    // 取得できなかった関連語は、空の見出しも失敗文も出さない
    expect(x.google.paragraphs, tag('Google 関連語は出さない')).not.toContain('関連キーワードを取得できませんでした。');
    expect(x.google.tables, tag('Google に検索数の表を作っていない')).toEqual([]);
  } else {
    expect(x.google.paragraphs, tag('Google 未取得')).toContain(
      '検索需要を取得するには検索語を指定し、Google広告APIの取得を選択してください。',
    );
  }
  expect(x.indeed.paragraphs, tag('Indeed')).toContain(
    '選択した職種・地域のIndeedデータがありません。職種を指定しているか確認してください。',
  );
  expect(x.indeed.tables, tag('Indeed に表なし')).toEqual([]);
  if (c.prefecture === '') {
    // 全国は市区町村の人口を合算して出す。E2E には外部統計 (Turso) が無いので「取得できませんでした」
    expect(x.population.paragraphs, tag('人口(全国)')).toEqual(['集計地域：全国', '全国の人口データを取得できませんでした。']);
    expect(x.population.tables, tag('人口(全国)に表なし')).toEqual([]);
  } else {
    // 最低賃金は公式 CSV (data/minimum_wage_rates.csv: 2026,大阪府,1231,2026-10-01)。年齢別人口と労働統計は
    // 外部統計 (Turso) が E2E に無いので「データなし」「—」。基準日は今日 (日本時間)。
    expect(x.population.paragraphs, tag('人口の地域')).toContain('集計地域：大阪府');
    expect(x.population.paragraphs, tag('人口の基準日')).toContain('人口の基準日：未取得');
    expect(x.population.tables, tag('人口の表')).toEqual([
      [['年齢', '男性', '女性', '合計'], ['データなし']],
      [
        ['最低賃金（円/時）', '1,231'],
        ['最低賃金の改定年度', '2026'],
        ['最低賃金の発効日', '2026-10-01'],
        ['最低賃金の基準日（日本時間）', jstToday()],
        ['最低賃金の出典', '厚生労働省の公式改定一覧 / 公式資料を確認'],
        ['労働統計の年度', '—'],
        ['完全失業率（%）', '—'],
        ['離職率（%）', '—'],
      ],
    ]);
  }
}

// ---------------------------------------------------------------- テスト

test.describe('競合調査 旧画面 == 新画面 == 既知値', () => {
  for (const [name, c] of [
    ['月給・大阪府・Google あり', CASE_MONTHLY],
    ['時給・全国・Google なし', CASE_HOURLY],
  ] as const) {
    test(name, async ({ browser }) => {
      // 旧と新は別のコンテキスト (sessionStorage などを共有しない)
      const oldCtx = await browser.newContext();
      const newCtx = await browser.newContext();
      try {
        const oldPage = await oldCtx.newPage();
        const newPage = await newCtx.newPage();
        await login(oldPage);
        await login(newPage);
        const legacy = await legacyReport(oldPage, c);
        const app = await appReport(newPage, c);

        assertKnown('旧画面', legacy, c);
        assertKnown('新画面', app, c);
        // 既知値に載せていない部分 (棒の位置・高さ、注記、キャプション等) も旧 == 新
        expect(app).toEqual(legacy);
      } finally {
        await oldCtx.close();
        await newCtx.close();
      }
    });
  }
});

// ---------------------------------------------------------------- 新画面単独

async function openAppForm(page: Page): Promise<void> {
  await login(page);
  await page.goto('/app/competitor');
  await expect(page.locator('#csv-file')).toBeVisible();
  await page.locator('#prefecture').selectOption({ label: '大阪府' });
  await page.locator('input[name="include_google"]').uncheck();
  await page.locator('#top-n').fill(TOP_N);
  await page.locator('#csv-file').setInputFiles(CSV);
}

test.describe('競合調査 新画面 /app/competitor 単独', () => {
  test('二重クリックしてもレポート作成の POST は 1 回', async ({ page }) => {
    await openAppForm(page);
    let posts = 0;
    await page.route('**/api/competitor/report', async (route) => {
      posts += 1;
      await new Promise((r) => setTimeout(r, 400)); // 1 回目が終わる前に 2 回目が来る状況を作る
      await route.continue();
    });
    await page.getByRole('button', { name: 'レポートを作成' }).dblclick();
    await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
    expect(posts).toBe(1);
  });

  test('同じ tick の 2 回の submit (ボタンの無効化に頼らない) でも POST は 1 回', async ({ page }) => {
    await openAppForm(page);
    let posts = 0;
    await page.route('**/api/competitor/report', async (route) => {
      posts += 1;
      await route.continue();
    });
    await page.locator('form').evaluate((f) => {
      const form = f as HTMLFormElement;
      form.requestSubmit();
      form.requestSubmit();
    });
    await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
    expect(posts).toBe(1);
  });

  test('サーバが返したエラーコードと文言を表示し、入力は残って再試行できる', async ({ page }) => {
    await login(page);
    await page.goto('/app/competitor');
    await page.locator('#prefecture').selectOption({ label: '大阪府' });
    await page.locator('input[name="include_google"]').uncheck();
    // 給与列の無い CSV: ブラウザ側の検査 (空・拡張子・サイズ) は通り、サーバが解析できずエラーコードを返す
    await page.locator('#csv-file').setInputFiles({
      name: 'bad.csv', mimeType: 'text/csv', buffer: Buffer.from('a,b\n1,2\n'),
    });
    const responseEvent = page.waitForResponse((r) => r.url().endsWith('/api/competitor/report'));
    await page.getByRole('button', { name: 'レポートを作成' }).click();
    const response = await responseEvent;
    expect(response.status()).toBe(422);
    const body = (await response.json()) as { error: string; message: string };
    expect(['csv_parse_failed', 'no_indeed_jobs']).toContain(body.error);
    const alert = page.locator('main [role="alert"]');
    await expect(alert).toBeVisible();
    await expect(alert).toContainText(body.message);
    await expect(page.getByRole('button', { name: 'レポートを作成' })).toBeEnabled();
    await expect(page.locator('#prefecture')).toHaveValue('大阪府'); // 入力は残る
    // 同じ画面から正しい CSV で作り直せる
    await page.locator('#csv-file').setInputFiles(CSV);
    await page.getByRole('button', { name: 'レポートを作成' }).click();
    await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
    await expect(alert).toHaveCount(0);
  });

  test('PDF の混雑エラー (pdf_busy) はコード別の文言で表示し、ボタンは再び押せる', async ({ page }) => {
    await openAppForm(page);
    await page.getByRole('button', { name: 'レポートを作成' }).click();
    await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
    await page.route('**/api/competitor/pdf', (route) =>
      route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'pdf_busy', message: 'PDF作成が混み合っています。少し時間をおいて再度お試しください。' }),
      }),
    );
    await page.getByRole('button', { name: 'PDFをダウンロード' }).click();
    await expect(page.locator('main [role="alert"]')).toContainText('PDF作成が混み合っています');
    await expect(page.getByRole('button', { name: 'PDFをダウンロード' })).toBeEnabled();
  });

  test('PDF ボタンで /api/competitor/pdf が呼ばれ、実 PDF (%PDF- … %%EOF) を保存する', async ({ page }) => {
    await openAppForm(page);
    await page.getByRole('button', { name: 'レポートを作成' }).click();
    await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();

    const requests: string[] = [];
    page.on('request', (r) => {
      if (r.url().includes('/api/competitor/pdf')) requests.push(`${r.method()} ${new URL(r.url()).pathname}`);
    });
    const responseEvent = page.waitForResponse((r) => r.url().endsWith('/api/competitor/pdf'));
    const downloadEvent = page.waitForEvent('download', { timeout: 60_000 }).catch(() => null);
    await page.getByRole('button', { name: 'PDFをダウンロード' }).click();
    const response = await responseEvent;
    expect(requests).toEqual(['POST /api/competitor/pdf']);

    if (response.status() === 200) {
      expect(response.headers()['content-type']).toContain('application/pdf');
      const download = await downloadEvent;
      expect(download, 'ダウンロードが始まらない').not.toBeNull();
      expect(download!.suggestedFilename()).toBe(`競合調査_大阪府_大阪府_${jstToday()}.pdf`);
      const bytes = await readFile((await download!.path())!);
      expect(bytes.subarray(0, 5).toString()).toBe('%PDF-');
      expect(bytes.subarray(Math.max(0, bytes.length - 32)).toString('latin1')).toContain('%%EOF');
      expect(bytes.length).toBeGreaterThan(10_000);
      await expect(page.getByText('PDFをダウンロードしました')).toBeVisible();
    } else {
      // Chromium が無い環境: 失敗が画面に出ること。CI (PDF_CHROMIUM_PATH あり) ではここに来てはいけない。
      expect(process.env.CI, `CI なのに PDF が ${String(response.status())}`).toBeUndefined();
      await expect(page.locator('main [role="alert"]')).toBeVisible();
    }
    await expect(page.getByRole('button', { name: 'PDFをダウンロード' })).toBeEnabled();
  });
});
