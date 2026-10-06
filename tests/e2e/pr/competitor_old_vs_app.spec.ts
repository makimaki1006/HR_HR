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
 * 判定は 旧 == 新 == 既知値 の 3 点。既知値 (helpers/fixture_values.ts の COMPETITOR_FIXTURE) は
 * scripts/e2e/competitor_expected.py が CSV から Rust と無関係に計算した値で、Rust の出力のコピーではない。
 *
 * 外部 API は呼ばない: global-setup.ts が GOOGLE_* 等を子プロセスの環境から除くので、Google は常に
 * 資格情報なし (missing_credentials) の経路。Indeed 採用市場 DB は E2E の環境に無い (unavailable)。
 * Google の ok / error / timeout の表示は Rust の契約テストと React の単体テストで確かめている。
 */

const CSV = path.resolve(__dirname, '../../fixtures/competitor/sp_utf8.csv');
const TITLE = 'E2E 大阪府・施設長';
const TOP_N = '10';

interface Conditions {
  title: string;
  prefecture: string;
  wageMode: 'monthly' | 'hourly';
  google: boolean;
  keyword: string;
}

const CASE_MONTHLY: Conditions = {
  title: TITLE, prefecture: '大阪府', wageMode: 'monthly', google: true, keyword: '施設長 求人',
};
const CASE_HOURLY: Conditions = {
  title: '', prefecture: '', wageMode: 'hourly', google: false, keyword: '',
};

// ---------------------------------------------------------------- 画面から値を取り出す (旧・新で同じ関数)

interface Extracted {
  meta: string[][];
  headings: string[];
  salary: string[][];
  diff: string[][];
  wordsAll: string[][];
  wordsHead: string[][];
  asideNotes: string[];
  caption: string[];
  charts: { caption: string; bars: string[]; axis: string[]; geometry: string[] }[];
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
    if (tables.length !== 5) throw new Error(`集計表が ${String(tables.length)} 枚 (期待 5)`);
    const r1 = (n: number): string => (Math.round(n * 10) / 10).toFixed(1);
    const charts = Array.from(excel.querySelectorAll('figure')).map((f) => ({
      caption: norm(f.querySelector('figcaption')?.textContent),
      bars: Array.from(f.querySelectorAll('rect > title')).map((t) => norm(t.textContent)),
      axis: Array.from(f.querySelectorAll('svg > text, svg > g > text')).map((t) => norm(t.textContent)),
      // 棒の位置と高さ (小数 1 桁): 件数が同じでも棒の長さが旧と違えば検知する
      geometry: Array.from(f.querySelectorAll('rect')).map(
        (r) => `${r1(Number(r.getAttribute('x')))}/${r1(Number(r.getAttribute('height')))}`,
      ),
    }));
    return {
      meta: rowsOf(tables[0]!),
      headings: Array.from(aside.querySelectorAll('h2'))
        .map((h) => norm(h.textContent))
        .filter((t) => t !== '競合調査'),
      salary: rowsOf(tables[1]!),
      diff: rowsOf(tables[2]!),
      wordsAll: rowsOf(tables[3]!),
      wordsHead: rowsOf(tables[4]!),
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

async function fillCommon(page: Page, c: Conditions): Promise<void> {
  await page.locator('#survey-title').fill(c.title);
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
  await fillCommon(page, c);
  await page.getByRole('button', { name: '画面で確認' }).click();
  await expect(page).toHaveURL(/\/report\/competitor$/);
  await expect(page.locator('#panel-excel')).toBeVisible();
  return extract(page);
}

async function appReport(page: Page, c: Conditions): Promise<Extracted> {
  await page.goto('/app/competitor');
  await expect(page.locator('#csv-file')).toBeVisible();
  await fillCommon(page, c);
  await page.getByRole('button', { name: 'レポートを作成' }).click();
  await expect(page.locator('[data-testid="result-summary"]')).toBeVisible();
  await expect(page.locator('#panel-excel')).toBeVisible();
  return extract(page);
}

// ---------------------------------------------------------------- 既知値 (fixture から別途確定した値)

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

function knownWords(rows: readonly (readonly [string, number, number, string])[]): string[][] {
  return [
    ['上位10件', '件数', '求人数', '占有率'],
    ...rows.map(([w, n, jobs, pct]) => [w, String(n), String(jobs), `${pct}%`]),
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
    [c.title === '' ? 'Indeed競合調査' : c.title, '正社員'],
    ['該当都道府県', '主な市町村'],
    ['大阪府', '大阪市'],
    ['集計対象', '該当件数'],
    ['CSV重複排除後', String(K.total)],
  ]);
  expect(x.headings, tag('見出し')).toEqual([
    `給与関係（${unit}）`,
    '差異（総合 − 人気求人）',
    '求人票ワード調査（全体）',
    '求人票ワード調査（上位 10 件）',
  ]);
  expect(x.salary, tag('給与表')).toEqual(knownSalary(mode));
  expect(x.diff, tag('差異表')).toEqual(knownDiff(mode));
  expect(x.wordsAll, tag('ワード表(全体)')).toEqual(knownWords(K.keywords_all));
  expect(x.wordsHead, tag('ワード表(上位 10 件)')).toEqual(knownWords(K.keywords_head));

  const [upper, lower, wordsAll, wordsHead] = x.charts;
  expect(x.charts.map((ch) => ch.caption), tag('グラフの題')).toEqual([
    `上限ボリュームゾーン（${unit}）`,
    `下限ボリュームゾーン（${unit}）`,
    '求人票キーワード調査（全体）',
    '求人票キーワード調査（上位 10 件）',
  ]);
  expect(upper!.bars, tag('上限ヒストグラム')).toEqual(knownBars(K[mode].hist_upper));
  expect(lower!.bars, tag('下限ヒストグラム')).toEqual(knownBars(K[mode].hist_lower));
  expect(wordsAll!.bars, tag('キーワード棒(全体)')).toEqual(knownBars(K.keywords_all.map(([w, n]) => [w, n] as const)));
  expect(wordsHead!.bars, tag('キーワード棒(上位 10 件)')).toEqual(
    knownBars(K.keywords_head.map(([w, n]) => [w, n] as const)),
  );
  // ヒストグラムの件数の合計は、月給モードは換算後の全件 (60)、時給モードは時給の行 (16)
  const total = (bins: readonly (readonly [string, number])[]): number => bins.reduce((a, [, n]) => a + n, 0);
  expect(total(K[mode].hist_upper), tag('上限ヒストグラムの合計')).toBe(mode === 'monthly' ? K.total : K.hourly.counts[0]);
  expect(total(K[mode].hist_lower), tag('下限ヒストグラムの合計')).toBe(mode === 'monthly' ? K.total : K.hourly.counts[0]);

  // Google / Indeed / 人口の状態 (CI は外部 API 未設定。Indeed は職種を選ばないので「データがありません」の文言)
  if (c.google) {
    expect(x.google.paragraphs, tag('Google')).toContain(`検索語: ${c.keyword} / 指定地域: ${c.prefecture}`);
    expect(x.google.paragraphs, tag('Google 需要の失敗文')).toContain(
      'Google検索需要を取得できませんでした。API設定または接続状況を確認してください。',
    );
    expect(x.google.paragraphs, tag('Google 関連語の失敗文')).toContain('関連キーワードを取得できませんでした。');
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
    expect(x.population.paragraphs, tag('人口(全国)')).toContain(
      '人口・地域データを表示するには、入力画面で対象都道府県を選択してください。',
    );
    expect(x.population.tables, tag('人口(全国)に表なし')).toEqual([]);
  } else {
    // 最低賃金は公式 CSV (data/minimum_wage_rates.csv: 2026,大阪府,1231,2026-10-01)。年齢別人口と労働統計は
    // 外部統計 (Turso) が E2E に無いので「データなし」「—」。基準日は今日 (日本時間)。
    expect(x.population.paragraphs, tag('人口の地域')).toContain('集計地域：大阪府');
    expect(x.population.tables, tag('人口の表')).toEqual([
      [['年齢', '男性', '女性', '合計'], ['データなし']],
      [
        ['最低賃金（円/時）', '1,231'],
        ['最低賃金の改定年度', '2026'],
        ['最低賃金の発効日', '2026-10-01'],
        ['最低賃金の基準日（日本時間）', jstToday()],
        ['最低賃金の出典', '厚生労働省の公式改定一覧'],
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
      expect(download!.suggestedFilename()).toBe(`競合調査_大阪府_Indeed競合調査_${jstToday()}.pdf`);
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
