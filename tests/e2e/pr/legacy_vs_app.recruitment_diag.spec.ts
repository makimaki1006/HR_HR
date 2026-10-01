import { Browser, expect, Locator, Page, test } from '@playwright/test';
import { PR_BASE_URL, RD_FIXTURE as RD } from './helpers/fixture_values';
import { serveLegacyCdnLocally } from './helpers/legacy_cdn';
import { getChartSeriesLengths, login } from './helpers/login';

// 旧シェルの CDN (htmx / ECharts) はローカルの同版ファイルで返す (helpers/legacy_cdn.ts)。
test.beforeEach(async ({ context }) => {
  await serveLegacyCdnLocally(context);
});

/**
 * 採用診断の旧新一致 spec (Phase 1A-5)。
 *   旧画面: /?tab=/tab/recruitment_diag  (旧シェル内に templates/tabs/recruitment_diag.html を表示)
 *   新画面: /app/recruitment-diag         (React)
 * 同じ条件 (飲食業 / 正社員 / 東京都 / 千代田区、自社 月給 28 万・年休 125・賞与 2.5) で診断を実行し、
 * 各パネルの表示値を 旧 == 新 == fixture の既知値 (helpers/fixture_values.ts の RD_FIXTURE) で比べる。
 * 要素の有無ではなく数値・文字列で判定する。計算過程は scripts/e2e/make_fixture_db.py の docstring。
 *
 * 意図的に旧画面と変える点 (比較から外す):
 *   - Panel 8 の重要度の色 (新は severity_rank で色分け) → 色は読まない。
 *   - 未選択で実行したときの案内 (旧は alert、新は画面内メッセージ) → 別テストで個別に確かめる。
 *   - Panel 3 は旧新とも「開発中」表示 → 両方とも「開発中」であることだけを比べる。
 * Turso / SalesNow が無い環境なので Panel 4 / Panel 6 は旧新とも「エラー」状態。件数などの検証はできない。
 *
 * 新画面の data-testid は規約 (実装担当と合意) に従う:
 *   パネル rd-panel-{name} (data-status = idle|loading|error|done)、状態文言 rd-panel-{name}-status、
 *   表示値 rd-{name}-{API のフィールドパス、ドットは -}、配列の行 rd-{name}-row、
 *   フォーム rd-form-*、実行 rd-run、グラフ rd-chart-trend / rd-chart-opportunity。
 * 表示値の書式は旧画面と同じ前提 (年収は「350万」、シェアは「25.00」など)。数値は先頭の数字だけを取り出して比べる。
 */

type Panel =
  | 'difficulty'
  | 'talent_pool'
  | 'inflow'
  | 'competitors'
  | 'condition_gap'
  | 'market_trend'
  | 'opportunity_map'
  | 'insights'
  | 'talent_pool_expansion';
const PANELS: Panel[] = [
  'difficulty',
  'talent_pool',
  'inflow',
  'competitors',
  'condition_gap',
  'market_trend',
  'opportunity_map',
  'insights',
  'talent_pool_expansion',
];

/** 旧新で同じ形に正規化した、画面の表示値。 */
interface Snapshot {
  states: Record<Panel, 'done' | 'error'>;
  /** 件数入りの状態文言 (旧画面と同じ書式の前提)。 */
  statusTexts: { inflow: string; opportunity_map: string; insights: string; talent_pool_expansion: string };
  difficulty: {
    score: string;
    rankLabel: string;
    hwCount: string;
    population: string;
    sharePct: string;
    nationalHwCount: string;
    soWhat: string;
  };
  talentPool: { day: string; night: string; inflow: string; ratio: string };
  inflowShowsDeveloping: boolean;
  competitorsErrorShown: boolean;
  conditionGap: {
    industry: { n: string; annual: string; holidays: string; bonus: string };
    allIndustry: { n: string; annual: string; holidays: string; bonus: string };
    interpretation: string;
  };
  marketTrendErrorShown: boolean;
  trendChartPresent: boolean;
  opportunity: { seriesLength: number; scoresSorted: number[] };
  /** 各行に 示唆 ID / 見出し / 本文 / アクションが全部含まれるか (順番どおり)。 */
  insights: { count: number; ids: string[] };
  expansion: {
    tier30: { count: string; pool: string; hw: string };
    tier60: { count: string; pool: string; hw: string };
    rows: { name: string; nums: string[] }[];
  };
}

// ---------- 共通ユーティリティ ----------

const collapse = (s: string | null | undefined): string => (s ?? '').replace(/\s+/g, ' ').trim();

/** 先頭の数値 (カンマを除き、符号 + / - は残す。Panel 2・9 の「+」も比べるため)。数値が無ければ空文字。 */
function num(text: string | null | undefined): string {
  const m = (text ?? '').replace(/,/g, '').match(/[+-]?\d+(?:\.\d+)?/);
  return m ? m[0] : '';
}

/** 文字列中の数値を全部 (カンマ・先頭の + を除いて) 返す。 */

function expectedSnapshot(): Snapshot {
  const d = RD.difficulty;
  const g = RD.conditionGap.display;
  const x = RD.expansion;
  return {
    states: {
      difficulty: 'done',
      talent_pool: 'done',
      inflow: 'done',
      competitors: 'error',
      condition_gap: 'done',
      market_trend: 'error',
      opportunity_map: 'done',
      insights: 'done',
      talent_pool_expansion: 'done',
    },
    statusTexts: {
      inflow: '開発中',
      opportunity_map: `完了（${RD.opportunity.count}件）`,
      insights: `完了（${RD.insights.length}件）`,
      talent_pool_expansion: x.statusText,
    },
    difficulty: {
      score: d.scoreDisplay,
      rankLabel: d.rankLabel,
      hwCount: String(d.hwCount),
      population: String(d.dayPopulation),
      sharePct: d.areaSharePctDisplay,
      nationalHwCount: String(d.nationalHwCount),
      soWhat: d.soWhat,
    },
    talentPool: {
      day: String(RD.talentPool.day),
      night: String(RD.talentPool.night),
      inflow: `+${RD.talentPool.inflow}`, // 旧新とも 0 以上は「+」付き
      ratio: String(RD.talentPool.ratio),
    },
    inflowShowsDeveloping: true,
    competitorsErrorShown: true,
    conditionGap: {
      industry: g.industry,
      allIndustry: g.allIndustry,
      interpretation: RD.conditionGap.interpretation,
    },
    marketTrendErrorShown: true,
    trendChartPresent: false,
    opportunity: {
      seriesLength: RD.opportunity.count,
      scoresSorted: RD.opportunity.municipalities.map((m) => m.score).sort((a, b) => a - b),
    },
    insights: { count: RD.insights.length, ids: RD.insights.map((i) => i.id) },
    expansion: {
      tier30: { count: String(x.tier30.count), pool: `+${x.tier30.unemploymentPool}`, hw: `+${x.tier30.hwPostings}` },
      tier60: { count: String(x.tier60.count), pool: `+${x.tier60.unemploymentPool}`, hw: `+${x.tier60.hwPostings}` },
      rows: x.rows.map((r) => ({ name: r[0], nums: [String(r[1]), String(r[2]), String(r[3])] })),
    },
  };
}

/** 示唆 1 件の表示テキストに ID / 見出し / 本文 / アクションが全部入っているか。入っていれば ID、無ければ不足の説明。 */
function insightRowId(text: string, exp: (typeof RD.insights)[number]): string {
  const t = collapse(text);
  const missing = [exp.id, exp.title, collapse(exp.message), collapse(exp.action)].filter((s) => !t.includes(s));
  return missing.length === 0 ? exp.id : `${exp.id} (欠け: ${missing.join(' | ')}) in ${t}`;
}

async function fillOwn(locator: Locator, value: string): Promise<void> {
  // 自社条件は折りたたみ (details) の中。閉じていれば開く
  await locator.evaluate((el) => {
    const d = el.closest('details');
    if (d) (d as HTMLDetailsElement).open = true;
  });
  await locator.fill(value);
}

// ---------- 旧画面 ----------

async function runLegacy(page: Page): Promise<void> {
  await login(page);
  await page.goto('/?tab=' + encodeURIComponent('/tab/recruitment_diag'));
  await expect(page.locator('#rd-run-btn')).toBeVisible();
  await page.selectOption('#rd-job-type', { label: RD.jobType });
  await page.selectOption('#rd-emp-type', { label: RD.empType });
  await page.selectOption('#rd-pref', { label: RD.pref });
  // 市区町村は都道府県の選択後に /api/municipalities_cascade から非同期で入る
  await expect(page.locator('#rd-city option', { hasText: RD.city })).toHaveCount(1);
  await page.selectOption('#rd-city', { label: RD.city });
  await fillOwn(page.locator('#rd-own-salary'), String(RD.own.salaryMan));
  await fillOwn(page.locator('#rd-own-holidays'), String(RD.own.holidays));
  await fillOwn(page.locator('#rd-own-bonus'), String(RD.own.bonus));
  await page.click('#rd-run-btn');
  // 固定 sleep ではなく、全パネルの状態文言が「待機中」「取得中...」から落ち着くのを待つ
  for (const name of PANELS) {
    await expect(page.locator(`section[data-panel="${name}"] .rd-panel-status`)).toHaveText(
      /^(完了|取得失敗|開発中|データなし)/,
    );
  }
  await expect(page.locator('#rd-global-status')).toHaveText('診断完了');
}

async function readLegacy(page: Page): Promise<Snapshot> {
  const raw = await page.evaluate(() => {
    const t = (el: Element | null | undefined): string => ((el as HTMLElement | null)?.textContent ?? '').replace(/\s+/g, ' ').trim();
    const sec = (n: string) => document.querySelector(`section[data-panel="${n}"]`) as HTMLElement;
    const body = (n: string) => sec(n).querySelector('.rd-panel-body') as HTMLElement;
    const status = (n: string) => t(sec(n).querySelector('.rd-panel-status'));
    const names = [
      'difficulty', 'talent_pool', 'inflow', 'competitors', 'condition_gap', 'market_trend',
      'opportunity_map', 'insights', 'talent_pool_expansion',
    ];
    const statuses: Record<string, string> = {};
    const bodies: Record<string, string> = {};
    for (const n of names) { statuses[n] = status(n); bodies[n] = t(body(n)); }

    // Panel 1: 3 つのカード (スコア / 件数・人口 / 全国比)
    const dc = Array.from(body('difficulty').querySelectorAll('.grid > div'));
    const difficulty = {
      score: t(dc[0]?.querySelector('.text-3xl')),
      rankLabel: t(dc[0]?.querySelector('.text-sm')),
      hwCount: t(dc[1]?.querySelector('.text-xl')),
      population: t(dc[1]?.querySelector('.text-xs.mt-1')),
      share: t(dc[2]?.querySelector('.text-2xl')),
      national: t(dc[2]?.querySelector('.text-xs.mt-1')),
      soWhat: t(body('difficulty').querySelector('.border-blue-500')),
    };
    // Panel 2: 4 つのカードの .text-xl (昼 / 夜 / 差分 / 昼夜比)
    const talentPool = Array.from(body('talent_pool').querySelectorAll('.grid > div .text-xl')).map((e) => t(e));
    // Panel 5: 2 つの表 (業界 / 全業界)。各行は [項目, 中央値, 差]
    const gapBoxes = Array.from(body('condition_gap').querySelectorAll('.grid > div'));
    const gapBox = (b: Element | undefined) => ({
      n: t(b?.querySelector('h4')),
      rows: Array.from(b?.querySelectorAll('tbody tr') ?? []).map((tr) =>
        Array.from(tr.querySelectorAll('td')).map((td) => t(td)),
      ),
    });
    const conditionGap = {
      industry: gapBox(gapBoxes[0]),
      all: gapBox(gapBoxes[1]),
      interpretation: t(body('condition_gap').querySelector('.border-blue-500')),
    };
    // Panel 8: 1 示唆 = 1 カード
    const insights = Array.from(body('insights').querySelectorAll('.space-y-3 > div')).map((e) => t(e));
    // Panel 9: 上の 2 つの箱 (30 分圏 / 60 分圏) と、内訳の表 (30 分圏 → 60 分圏)
    const boxes = Array.from(body('talent_pool_expansion').querySelectorAll(':scope > div.grid > div')).map((b) => ({
      label: t(b.querySelector('.font-semibold')),
      values: Array.from(b.querySelectorAll('.text-xl')).map((e) => t(e)),
    }));
    const rows = Array.from(body('talent_pool_expansion').querySelectorAll('details tbody tr')).map((tr) =>
      Array.from(tr.children).map((td) => t(td)),
    );
    const w = window as unknown as {
      _rdCharts?: Record<string, { getOption: () => { series?: { data?: unknown[] }[] } }>;
    };
    const chart = w._rdCharts?.['rd-chart-opportunity'];
    const series = chart?.getOption().series ?? [];
    const chartValues = (series[0]?.data ?? []).map((d) =>
      typeof d === 'object' && d !== null ? (d as { value: number }).value : (d as number),
    );
    return {
      statuses, bodies, difficulty, talentPool, conditionGap, insights, boxes, rows,
      chartValues, hasOpportunityChart: !!chart,
      trendChartPresent: !!document.getElementById('rd-chart-trend'),
    };
  });

  const stateOf = (s: string): 'done' | 'error' => (/^取得失敗/.test(s) ? 'error' : 'done');
  const states = Object.fromEntries(PANELS.map((n) => [n, stateOf(raw.statuses[n])])) as Snapshot['states'];
  const gapDisp = (b: { n: string; rows: string[][] }) => ({
    n: num(b.n),
    annual: num(b.rows[0]?.[1]),
    holidays: num(b.rows[1]?.[1]),
    bonus: num(b.rows[2]?.[1]),
  });
  const insightIds = RD.insights.map((exp, i) => insightRowId(raw.insights[i] ?? '', exp));
  expect(raw.hasOpportunityChart, '旧画面の ECharts (CDN から読み込み) が初期化されていない').toBe(true);

  return {
    states,
    statusTexts: {
      inflow: raw.statuses.inflow,
      opportunity_map: raw.statuses.opportunity_map,
      insights: raw.statuses.insights,
      talent_pool_expansion: raw.statuses.talent_pool_expansion,
    },
    difficulty: {
      score: num(raw.difficulty.score),
      rankLabel: raw.difficulty.rankLabel,
      hwCount: num(raw.difficulty.hwCount),
      population: num(raw.difficulty.population),
      sharePct: num(raw.difficulty.share),
      nationalHwCount: num(raw.difficulty.national),
      soWhat: raw.difficulty.soWhat.replace(/^📝\s*/, ''),
    },
    talentPool: {
      day: num(raw.talentPool[0]),
      night: num(raw.talentPool[1]),
      inflow: num(raw.talentPool[2]),
      ratio: num(raw.talentPool[3]),
    },
    inflowShowsDeveloping: raw.bodies.inflow.includes('開発中'),
    competitorsErrorShown: raw.bodies.competitors.includes(RD.competitorsError),
    conditionGap: {
      industry: { ...gapDisp(raw.conditionGap.industry) },
      allIndustry: { ...gapDisp(raw.conditionGap.all) },
      interpretation: raw.conditionGap.interpretation.replace(/^📝\s*/, ''),
    },
    marketTrendErrorShown: raw.bodies.market_trend.includes(RD.marketTrendError),
    trendChartPresent: raw.trendChartPresent,
    opportunity: {
      seriesLength: raw.chartValues.length,
      scoresSorted: [...raw.chartValues].sort((a, b) => a - b),
    },
    insights: { count: raw.insights.length, ids: insightIds },
    expansion: {
      tier30: {
        count: raw.boxes[0]?.label.match(/上位\s*(\d+)/)?.[1] ?? '',
        pool: num(raw.boxes[0]?.values[0]),
        hw: num(raw.boxes[0]?.values[1]),
      },
      tier60: {
        count: raw.boxes[1]?.label.match(/上位\s*(\d+)/)?.[1] ?? '',
        pool: num(raw.boxes[1]?.values[0]),
        hw: num(raw.boxes[1]?.values[1]),
      },
      rows: raw.rows.map((cells) => ({ name: collapse(cells[0]), nums: cells.slice(1).map((c) => num(c)) })),
    },
  };
}

/** 旧画面: Panel 5 の「自社との差」(符号つき)。新画面は readAppGaps (rd-condition_gap-{gap_industry|gap_all}-*) で同じ値を取る。 */
async function readLegacyGaps(page: Page): Promise<string[][]> {
  return page.evaluate(() => {
    const t = (el: Element | null | undefined): string => ((el as HTMLElement | null)?.textContent ?? '').replace(/\s+/g, ' ').trim();
    const boxes = Array.from(document.querySelectorAll('section[data-panel="condition_gap"] .rd-panel-body .grid > div'));
    return boxes.map((b) => Array.from(b.querySelectorAll('tbody tr')).map((tr) => t(tr.querySelectorAll('td')[2])));
  });
}

// ---------- 新画面 ----------

/** 新画面の「自社との差」(業界 / 全業界 × 年収・年休・賞与)。testid は rd-condition_gap-{gap_industry|gap_all}-{項目}。 */
async function readAppGaps(page: Page): Promise<string[][]> {
  const out: string[][] = [];
  for (const key of ['gap_industry', 'gap_all']) {
    const row: string[] = [];
    for (const f of ['annual_income', 'annual_holidays', 'bonus_months']) {
      row.push(collapse(await page.getByTestId(`rd-condition_gap-${key}-${f}`).textContent()));
    }
    out.push(row);
  }
  return out;
}

const tid = (page: Page, id: string) => page.getByTestId(id);
const tidText = async (page: Page, id: string): Promise<string> => collapse(await tid(page, id).textContent());

async function runApp(page: Page): Promise<void> {
  await login(page);
  await page.goto('/app/recruitment-diag');
  await expect(tid(page, 'rd-run')).toBeVisible();
  await tid(page, 'rd-form-job-type').selectOption({ label: RD.jobType });
  await tid(page, 'rd-form-emp-type').selectOption({ label: RD.empType });
  await tid(page, 'rd-form-pref').selectOption({ label: RD.pref });
  await expect(tid(page, 'rd-form-city').locator('option', { hasText: RD.city })).toHaveCount(1);
  await tid(page, 'rd-form-city').selectOption({ label: RD.city });
  await fillOwn(tid(page, 'rd-form-own-salary'), String(RD.own.salaryMan));
  await fillOwn(tid(page, 'rd-form-own-holidays'), String(RD.own.holidays));
  await fillOwn(tid(page, 'rd-form-own-bonus'), String(RD.own.bonus));
  await tid(page, 'rd-run').click();
  for (const name of PANELS) {
    await expect(tid(page, `rd-panel-${name}`)).toHaveAttribute('data-status', /^(done|error)$/);
  }
}

/** ECharts インスタンスの series[0] の値 (EChart 部品の data-chart-ready を待ってから取り出す)。 */
async function getChartSeriesValues(page: Page, testId: string): Promise<number[]> {
  const el = page.locator(`[data-testid="${testId}"][data-chart-ready="true"]`);
  await expect(el).toHaveCount(1);
  return el.evaluate((dom) => {
    const w = window as unknown as {
      __echarts_getInstanceByDom?: (d: HTMLElement) =>
        | { getOption: () => { series?: { data?: unknown[] }[] } }
        | undefined;
    };
    const inst = w.__echarts_getInstanceByDom?.(dom as HTMLElement);
    if (!inst) throw new Error('echarts instance not found');
    return ((inst.getOption().series ?? [])[0]?.data ?? []).map((d) =>
      typeof d === 'object' && d !== null ? (d as { value: number }).value : (d as number),
    );
  });
}

async function readApp(page: Page): Promise<Snapshot> {
  const states = {} as Snapshot['states'];
  for (const n of PANELS) {
    const st = await tid(page, `rd-panel-${n}`).getAttribute('data-status');
    if (st !== 'done' && st !== 'error') throw new Error(`rd-panel-${n} の data-status が done / error でない: ${String(st)}`);
    states[n] = st;
  }
  const T = (id: string) => tidText(page, id);

  // Panel 8: 行ごとに ID / 見出し / 本文 / アクションが入っているか
  const insightRows = tid(page, 'rd-insights-row');
  const insightCount = await insightRows.count();
  const insightIds: string[] = [];
  for (let i = 0; i < Math.min(insightCount, RD.insights.length); i++) {
    insightIds.push(insightRowId((await insightRows.nth(i).textContent()) ?? '', RD.insights[i]));
  }

  // Panel 9: 行はセルごとに子要素になっている前提 (旧画面の td と同じ)。子要素が無ければ全体を 1 セルとして扱う
  const expRows = tid(page, 'rd-talent_pool_expansion-row');
  const expCount = await expRows.count();
  const rows: Snapshot['expansion']['rows'] = [];
  for (let i = 0; i < expCount; i++) {
    const cells: string[] = await expRows.nth(i).evaluate((el) => {
      const kids = Array.from(el.children);
      return (kids.length > 0 ? kids : [el]).map((k) => (k.textContent ?? '').replace(/\s+/g, ' ').trim());
    });
    rows.push({ name: cells[0] ?? '', nums: cells.slice(1).map((c) => num(c)) });
  }

  const opportunityLengths = await getChartSeriesLengths(page, 'rd-chart-opportunity');
  const opportunityValues = await getChartSeriesValues(page, 'rd-chart-opportunity');
  const errCompetitors = await tid(page, 'rd-panel-competitors').textContent();
  const errTrend = await tid(page, 'rd-panel-market_trend').textContent();
  const inflowText = await tid(page, 'rd-panel-inflow').textContent();

  return {
    states,
    statusTexts: {
      inflow: await T('rd-panel-inflow-status'),
      opportunity_map: await T('rd-panel-opportunity_map-status'),
      insights: await T('rd-panel-insights-status'),
      talent_pool_expansion: await T('rd-panel-talent_pool_expansion-status'),
    },
    difficulty: {
      score: num(await T('rd-difficulty-metrics-score_per_10k')),
      rankLabel: await T('rd-difficulty-rank_label'),
      hwCount: num(await T('rd-difficulty-metrics-hw_count')),
      population: num(await T('rd-difficulty-metrics-population')),
      sharePct: num(await T('rd-difficulty-metrics-area_share_of_national')),
      nationalHwCount: num(await T('rd-difficulty-metrics-national_hw_count')),
      soWhat: (await T('rd-difficulty-so_what')).replace(/^📝\s*/, ''),
    },
    talentPool: {
      day: num(await T('rd-talent_pool-metrics-day_population')),
      night: num(await T('rd-talent_pool-metrics-night_population')),
      inflow: num(await T('rd-talent_pool-metrics-commuter_inflow')),
      ratio: num(await T('rd-talent_pool-metrics-day_night_ratio')),
    },
    inflowShowsDeveloping: collapse(inflowText).includes('開発中'),
    competitorsErrorShown: collapse(errCompetitors).includes(RD.competitorsError),
    conditionGap: {
      industry: {
        n: num(await T('rd-condition_gap-industry_median-sample_size')),
        annual: num(await T('rd-condition_gap-industry_median-annual_income')),
        holidays: num(await T('rd-condition_gap-industry_median-annual_holidays')),
        bonus: num(await T('rd-condition_gap-industry_median-bonus_months')),
      },
      allIndustry: {
        n: num(await T('rd-condition_gap-all_industry_median-sample_size')),
        annual: num(await T('rd-condition_gap-all_industry_median-annual_income')),
        holidays: num(await T('rd-condition_gap-all_industry_median-annual_holidays')),
        bonus: num(await T('rd-condition_gap-all_industry_median-bonus_months')),
      },
      interpretation: (await T('rd-condition_gap-interpretation')).replace(/^📝\s*/, ''),
    },
    marketTrendErrorShown: collapse(errTrend).includes(RD.marketTrendError),
    trendChartPresent: (await page.getByTestId('rd-chart-trend').count()) > 0,
    opportunity: {
      seriesLength: opportunityLengths[0] ?? -1,
      scoresSorted: [...opportunityValues].sort((a, b) => a - b),
    },
    insights: { count: insightCount, ids: insightIds },
    expansion: {
      tier30: {
        count: num(await T('rd-talent_pool_expansion-tier_30min-municipality_count')),
        pool: num(await T('rd-talent_pool_expansion-tier_30min-unemployment_pool')),
        hw: num(await T('rd-talent_pool_expansion-tier_30min-hw_postings')),
      },
      tier60: {
        count: num(await T('rd-talent_pool_expansion-tier_60min-municipality_count')),
        pool: num(await T('rd-talent_pool_expansion-tier_60min-unemployment_pool')),
        hw: num(await T('rd-talent_pool_expansion-tier_60min-hw_postings')),
      },
      rows,
    },
  };
}

/** 新画面は別のブラウザコンテキスト (別セッション) で開く。旧画面の操作と cookie / ヘッダー絞り込みを共有しない。 */
async function withAppPage<T>(browser: Browser, fn: (page: Page) => Promise<T>): Promise<T> {
  const ctx = await browser.newContext({ baseURL: test.info().project.use.baseURL });
  try {
    return await fn(await ctx.newPage());
  } finally {
    await ctx.close();
  }
}


// ---------- 色・並び順 (旧 == 新 == 既知値) ----------

/** 色の区分は Tailwind の文字色クラス (旧画面と新画面で同じクラス名を使う) で比べる。 */
type Marks = {
  rankTone: string;
  inflowTone: string;
  /** Panel 7 の棒: 表示順 (下から上ではなく option の並び) の名前・値・色 */
  opportunityBars: { name: string; value: number; color: string }[];
};

const toneOf = (cls: string | null | undefined): string => (cls ?? '').match(/\btext-(?:red|orange|yellow|green|blue|slate)-\d{3}\b/)?.[0] ?? '';

const CATEGORY_COLOR: Record<string, string> = { 穴場: '#22c55e', 激戦: '#ef4444' };

function expectedMarks(): Marks {
  return {
    rankTone: 'text-green-400', // rank_label「穏やか」(旧画面 renderDifficulty の levelColor)
    inflowTone: 'text-blue-400', // commuter_inflow >= 0
    // 旧画面は score 昇順 (穴場が上位) に並べ、区分で色を付ける
    opportunityBars: [...RD.opportunity.municipalities]
      .sort((a, b) => a.score - b.score)
      .map((m) => ({ name: m.name, value: m.score, color: CATEGORY_COLOR[m.category] ?? '#64748b' })),
  };
}

type BarOption = { yAxis?: { data?: string[] }[]; series?: { data?: { value: number; itemStyle?: { color?: string } }[] }[] };

function barsOf(opt: BarOption | undefined): Marks['opportunityBars'] {
  const names = opt?.yAxis?.[0]?.data ?? [];
  const data = opt?.series?.[0]?.data ?? [];
  return data.map((d, i) => ({ name: names[i] ?? '', value: d.value, color: d.itemStyle?.color ?? '' }));
}

async function readLegacyMarks(page: Page): Promise<Marks> {
  const raw = await page.evaluate(() => {
    const body = (n: string) => document.querySelector(`section[data-panel="${n}"] .rd-panel-body`) as HTMLElement;
    const dc = body('difficulty').querySelectorAll('.grid > div');
    const tp = body('talent_pool').querySelectorAll('.grid > div .text-xl');
    const w = window as unknown as { _rdCharts?: Record<string, { getOption: () => unknown }> };
    return {
      rankCls: (dc[0]?.querySelector('.text-sm') as HTMLElement | null)?.className ?? '',
      inflowCls: (tp[2] as HTMLElement | undefined)?.className ?? '',
      opt: w._rdCharts?.['rd-chart-opportunity']?.getOption(),
    };
  });
  return { rankTone: toneOf(raw.rankCls), inflowTone: toneOf(raw.inflowCls), opportunityBars: barsOf(raw.opt as BarOption) };
}

async function readAppMarks(page: Page): Promise<Marks> {
  const rankCls = await page.getByTestId('rd-difficulty-rank_label').getAttribute('class');
  const inflowCls = await page.getByTestId('rd-talent_pool-metrics-commuter_inflow').getAttribute('class');
  const opt = await page.getByTestId('rd-chart-opportunity').evaluate((el) => {
    const w = window as unknown as { __echarts_getInstanceByDom?: (d: HTMLElement) => { getOption: () => unknown } | undefined };
    return w.__echarts_getInstanceByDom?.(el as HTMLElement)?.getOption();
  });
  return { rankTone: toneOf(rankCls), inflowTone: toneOf(inflowCls), opportunityBars: barsOf(opt as BarOption) };
}

// ---------- テスト ----------

test.describe('採用診断: 旧画面と新画面の値一致', () => {
  test('旧画面の表示値が fixture の既知値と一致する', async ({ page }) => {
    await runLegacy(page);
    expect(await readLegacy(page)).toEqual(expectedSnapshot());
    // 旧画面のみ: 自社との差の表示 (業界 / 全業界)。年収 +56 万 (= 406 万 - 350 万)、年休 +5 日、賞与 +0.5 ヶ月
    expect(await readLegacyGaps(page)).toEqual([RD.conditionGap.display.gaps, RD.conditionGap.display.gaps]);
  });

  test('新画面の表示値が fixture の既知値と一致する', async ({ page }) => {
    await runApp(page);
    expect(await readApp(page)).toEqual(expectedSnapshot());
    expect(await readAppGaps(page)).toEqual([RD.conditionGap.display.gaps, RD.conditionGap.display.gaps]);
  });

  test('旧画面 == 新画面 (全パネルの表示値)', async ({ page, browser }) => {
    await runLegacy(page);
    const legacy = await readLegacy(page);
    const app = await withAppPage(browser, async (appPage) => {
      await runApp(appPage);
      return { ...(await readApp(appPage)), gaps: await readAppGaps(appPage) };
    });
    expect(app).toEqual({ ...legacy, gaps: await readLegacyGaps(page) });
  });

  test('色の区分 (Panel 1 ランク・Panel 2 差分) と Panel 7 の棒の並び順・色が 旧 == 新 == 既知値', async ({ page, browser }) => {
    await runLegacy(page);
    const legacy = await readLegacyMarks(page);
    expect(legacy).toEqual(expectedMarks());
    const app = await withAppPage(browser, async (appPage) => {
      await runApp(appPage);
      await expect(appPage.getByTestId('rd-chart-opportunity')).toHaveAttribute('data-chart-ready', 'true');
      return readAppMarks(appPage);
    });
    expect(app).toEqual(legacy);
  });

  test('Panel 4 / Panel 6 は Turso・SalesNow 無しで旧新とも「エラー」になり、トレンドのグラフは出ない', async ({
    page,
    browser,
  }) => {
    // 件数などは検証できない (Turso 無し)。エラー状態と文言が旧新で一致することだけを確かめる
    await runLegacy(page);
    const legacy = await readLegacy(page);
    expect(legacy.states.competitors).toBe('error');
    expect(legacy.states.market_trend).toBe('error');
    expect(legacy.competitorsErrorShown).toBe(true);
    expect(legacy.marketTrendErrorShown).toBe(true);
    expect(legacy.trendChartPresent).toBe(false);
    const app = await withAppPage(browser, async (appPage) => {
      await runApp(appPage);
      return readApp(appPage);
    });
    expect(app.states.competitors).toBe('error');
    expect(app.states.market_trend).toBe('error');
    expect(app.competitorsErrorShown).toBe(true);
    expect(app.marketTrendErrorShown).toBe(true);
    expect(app.trendChartPresent).toBe(false);
  });
});

test.describe('採用診断: セッションのフィルタ', () => {
  test('セッションの市区町村が初期値になり、「すべて」を選ぶと都道府県全体で集計する (画面に無い市区町村で補わない)', async ({ page }) => {
    await login(page);
    // 旧シェルのヘッダーフィルタと同じ経路でセッションに 東京都 / 千代田区 を入れる
    const headers = { Origin: PR_BASE_URL, 'X-Requested-With': 'fetch' };
    expect((await page.request.post('/api/set_prefecture', { form: { prefecture: RD.pref }, headers })).ok()).toBe(true);
    expect((await page.request.post('/api/set_municipality', { form: { municipality: RD.city }, headers })).ok()).toBe(true);

    await page.goto('/app/recruitment-diag');
    await expect(tid(page, 'rd-form-pref')).toHaveValue(RD.pref);
    await expect(tid(page, 'rd-form-city')).toHaveValue(RD.city);

    await tid(page, 'rd-form-job-type').selectOption({ label: RD.jobType });
    await tid(page, 'rd-form-emp-type').selectOption({ label: RD.empType });
    await tid(page, 'rd-form-city').selectOption({ value: '' }); // すべて（都道府県全体）
    await expect(tid(page, 'rd-form-city')).toHaveValue('');
    await tid(page, 'rd-run').click();
    await expect(tid(page, 'rd-panel-difficulty')).toHaveAttribute('data-status', 'done');
    // fixture: 東京都 正社員 24 行のうち job_type が 飲食業 (k が奇数) は 千代田区 5 + 港区 4 + 新宿区 3 = 12 件。千代田区だけなら 5 件
    expect(num(await tidText(page, 'rd-difficulty-metrics-hw_count'))).toBe(String(RD.prefWideHwCount));

    // セッションの市区町村も空になっている (画面とセッションが食い違わない)
    const nav = await page.request.get('/api/filters/current', { headers: { Accept: 'application/json' } });
    expect(nav.ok()).toBe(true);
    expect(((await nav.json()) as { municipality: string }).municipality).toBe('');
  });
});

test.describe('採用診断: 未選択で実行したとき', () => {
  test('旧画面は alert、新画面は alert を出さずパネルも実行しない', async ({ page }) => {
    // 旧画面: 業種未選択 → alert('業種を選択してください')
    await login(page);
    await page.goto('/?tab=' + encodeURIComponent('/tab/recruitment_diag'));
    const dialogs: string[] = [];
    page.on('dialog', async (d) => {
      dialogs.push(d.message());
      await d.dismiss();
    });
    await page.click('#rd-run-btn');
    await expect.poll(() => dialogs).toEqual(['業種を選択してください']);

    // 新画面: alert ではなく画面内メッセージ (rd-form-message) に旧と同じ文言を出し、パネルは実行しない
    dialogs.length = 0;
    await page.goto('/app/recruitment-diag');
    await expect(tid(page, 'rd-run')).toBeVisible();
    await tid(page, 'rd-run').click();
    await expect(tid(page, 'rd-form-message')).toHaveText('業種を選択してください');
    expect(dialogs).toEqual([]);
    const done = await page.locator('[data-testid^="rd-panel-"][data-status="done"]').count();
    expect(done).toBe(0);
  });
});

test.describe('採用診断: 9 API の既知値 (画面ではなく API の JSON)', () => {
  const q = new URLSearchParams({
    job_type: RD.jobType,
    emp_type: RD.empType,
    prefecture: RD.pref,
    municipality: RD.city,
    prefcode: String(RD.prefcode),
    citycode: String(RD.citycode),
  }).toString();

  async function getJson(page: Page, path: string): Promise<any> {
    const res = await page.request.get(path, { headers: { Accept: 'application/json' } });
    expect(res.status()).toBe(200);
    return res.json();
  }

  test('Panel 1-3, 5, 7, 8, 9 は fixture の既知値、Panel 4 / 6 は Turso 無しのエラー', async ({ page }) => {
    await login(page);
    const d = await getJson(page, `/api/recruitment_diag/difficulty?${q}`);
    expect(d.inputs.citycode).toBe(RD.citycode);
    expect(d.metrics.hw_count).toBe(RD.difficulty.hwCount);
    expect(d.metrics.national_hw_count).toBe(RD.difficulty.nationalHwCount);
    expect(d.metrics.day_population).toBe(RD.difficulty.dayPopulation);
    expect(d.metrics.night_population).toBe(RD.difficulty.nightPopulation);
    expect(d.metrics.day_night_ratio).toBeCloseTo(RD.difficulty.dayNightRatio, 9);
    expect(d.metrics.is_tourist_area).toBe(false);
    expect(d.metrics.score_per_10k).toBeCloseTo(RD.difficulty.scorePer10k, 9);
    expect(d.metrics.area_share_of_national).toBeCloseTo(0.25, 9);
    expect(d.rank).toBe(RD.difficulty.rank);
    expect(d.rank_label).toBe(RD.difficulty.rankLabel);
    expect(d.so_what).toBe(RD.difficulty.soWhat);

    const tp = await getJson(page, `/api/recruitment_diag/talent_pool?${q}`);
    expect(tp.metrics.day_population).toBe(RD.talentPool.day);
    expect(tp.metrics.night_population).toBe(RD.talentPool.night);
    expect(tp.metrics.commuter_inflow).toBe(RD.talentPool.inflow);
    expect(tp.metrics.day_night_ratio).toBeCloseTo(RD.talentPool.ratio, 9);

    // 画面は「開発中」だが API は値を返す (from_area 0..3 の年合計、平日昼のみ)
    const inf = await getJson(page, `/api/recruitment_diag/inflow?${q}`);
    expect(inf.breakdown.map((b: any) => b.from_area)).toEqual([0, 1, 2, 3]);
    expect(inf.breakdown.map((b: any) => b.population)).toEqual(RD.inflow.populations);
    expect(inf.total_population).toBe(RD.inflow.total);
    inf.breakdown.forEach((b: any, i: number) => expect(b.share).toBeCloseTo(RD.inflow.shares[i], 9));

    const comp = await getJson(page, `/api/recruitment_diag/competitors?${q}&limit=100`);
    expect(comp.error).toBe(RD.competitorsError);

    const gapQ = new URLSearchParams(q);
    gapQ.set('company_salary_min', String(RD.own.salaryYen));
    gapQ.set('company_annual_holidays', String(RD.own.holidays));
    gapQ.set('company_bonus_months', String(RD.own.bonus));
    const g = await getJson(page, `/api/recruitment_diag/condition_gap?${gapQ.toString()}`);
    expect(g.industry_median).toEqual({
      annual_income: RD.conditionGap.industry.annualIncome,
      annual_holidays: RD.conditionGap.industry.annualHolidays,
      bonus_months: RD.conditionGap.industry.bonusMonths,
      sample_size: RD.conditionGap.industry.sampleSize,
    });
    expect(g.all_industry_median).toEqual({
      annual_income: RD.conditionGap.allIndustry.annualIncome,
      annual_holidays: RD.conditionGap.allIndustry.annualHolidays,
      bonus_months: RD.conditionGap.allIndustry.bonusMonths,
      sample_size: RD.conditionGap.allIndustry.sampleSize,
    });
    expect(g.company.annual_income_estimated).toBe(RD.conditionGap.ownAnnualIncome);
    expect(g.gap_industry.annual_income_diff).toBe(560000);
    expect(g.gap_industry.annual_income_pct).toBeCloseTo(16.0, 9);
    expect(g.gap_industry.annual_holidays_diff).toBe(5);
    expect(g.gap_industry.bonus_months_diff).toBe(0.5);
    expect(g.interpretation).toBe(RD.conditionGap.interpretation);

    const mt = await getJson(page, `/api/recruitment_diag/market_trend?${q}`);
    expect(mt.error).toBe(RD.marketTrendError);

    const op = await getJson(page, `/api/recruitment_diag/opportunity_map?${q}`);
    expect(op.municipalities.map((m: any) => [m.name, m.hw_count, m.population, m.score, m.category])).toEqual(
      RD.opportunity.municipalities.map((m) => [m.name, m.hwCount, m.population, m.score, m.category]),
    );

    const ins = await getJson(page, `/api/recruitment_diag/insights?${q}`);
    expect(ins.insights.map((i: any) => [i.pattern_id, i.title, i.message, i.hr_action])).toEqual(
      RD.insights.map((i) => [i.id, i.title, i.message, i.action]),
    );
    expect(ins.insights.map((i: any) => i.severity)).toEqual(['重大', '注意', '情報']);

    const ex = await getJson(page, `/api/recruitment_diag/talent_pool_expansion?${q}`);
    expect(ex.is_data_available).toBe(true);
    for (const [key, exp] of [['tier_30min', RD.expansion.tier30], ['tier_60min', RD.expansion.tier60]] as const) {
      expect(ex[key].municipality_count).toBe(exp.count);
      expect(ex[key].unemployment_pool).toBe(exp.unemploymentPool);
      expect(ex[key].hw_postings).toBe(exp.hwPostings);
    }
    const all = [...ex.tier_30min.breakdown, ...ex.tier_60min.breakdown];
    expect(all.map((r: any) => [`${r.prefecture} ${r.municipality}`, r.commuters, r.unemployment, r.hw_postings])).toEqual(
      RD.expansion.rows.map((r) => [...r]),
    );
  });
});
