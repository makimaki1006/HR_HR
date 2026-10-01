import { useMemo, type ReactNode } from 'react';
import { DataTable, EChart, Note, type DataTableColumn } from '../../components';
import type { RdCommuteTier } from '../../generated/RdCommuteTier';
import type { RdCompetitorItem } from '../../generated/RdCompetitorItem';
import type { RdCompetitorsResponse } from '../../generated/RdCompetitorsResponse';
import type { RdConditionGapResponse } from '../../generated/RdConditionGapResponse';
import type { RdDifficultyResponse } from '../../generated/RdDifficultyResponse';
import type { RdInsightsResponse } from '../../generated/RdInsightsResponse';
import type { RdMarketTrendResponse } from '../../generated/RdMarketTrendResponse';
import type { RdOpportunityMapResponse } from '../../generated/RdOpportunityMapResponse';
import type { RdTalentPoolExpansionResponse } from '../../generated/RdTalentPoolExpansionResponse';
import type { RdTalentPoolResponse } from '../../generated/RdTalentPoolResponse';
import { opportunityChartOption, trendChartOption } from './charts';
import { fmt, signPrefix, toManYen } from './format';

export type StatusTone = 'muted' | 'loading' | 'ok' | 'error';

export const TONE_CLASS: Record<StatusTone, string> = {
  muted: 'text-slate-400',
  loading: 'text-blue-300',
  ok: 'text-green-400',
  error: 'text-red-400',
};

/** What a panel shows once it has data: the status text next to the title and the body. */
export interface PanelView {
  status: 'done' | 'error';
  statusText: string;
  tone: StatusTone;
  body: ReactNode;
}

const done = (statusText: string, body: ReactNode, tone: StatusTone = 'ok'): PanelView => ({
  status: 'done',
  statusText,
  tone,
  body,
});

/** Old setPanelError(): status "取得失敗" + "❌ 取得失敗: <message>". */
export function errorView(message: string): PanelView {
  return {
    status: 'error',
    statusText: '取得失敗',
    tone: 'error',
    body: (
      <div
        className="rounded border border-red-600/50 bg-red-900/30 p-3 text-sm text-red-200"
        data-testid="rd-panel-error"
      >
        ❌ 取得失敗: {message}
      </div>
    ),
  };
}

const CARD = 'rounded border border-slate-700 bg-navy-900/60 p-3';
const SO_WHAT = 'mt-3 rounded border-l-2 border-blue-500 bg-navy-900/40 p-2 text-xs text-slate-300';
const EMPTY = 'py-4 text-sm text-slate-400';

/** Used where the old code tested Array.isArray(): a runtime shape check on parsed JSON. */
export function asArray(v: unknown): unknown[] | null {
  return Array.isArray(v) ? (v as unknown[]) : null;
}

// ---- Panel 1: difficulty ----

const RANK_COLOR: Record<string, string> = {
  非常に激戦: 'text-red-400',
  激戦: 'text-orange-400',
  平均的: 'text-yellow-300',
  穏やか: 'text-green-400',
};

export function rankColor(rankLabel: string): string {
  return RANK_COLOR[rankLabel] ?? (rankLabel.startsWith('穴場') ? 'text-blue-400' : 'text-slate-300');
}

export function renderDifficulty(d: RdDifficultyResponse): PanelView {
  const m = d.metrics;
  const rank = d.rank_label || '—';
  return done(
    '完了',
    <>
      <div className="mb-4 grid grid-cols-1 gap-4 sm:grid-cols-2 md:grid-cols-3">
        <div className={CARD}>
          <div className="text-xs text-slate-400">
            採用難度スコア <span className="text-[10px] text-slate-500">(昼人口1万人あたり求人数)</span>
          </div>
          <div className="text-3xl font-bold text-white" data-testid="rd-difficulty-metrics-score_per_10k">
            {fmt(m.score_per_10k, 1)}
          </div>
          <div className={`mt-1 text-sm ${rankColor(rank)}`} data-testid="rd-difficulty-rank_label">
            {rank}
          </div>
        </div>
        <div className={CARD}>
          <div className="text-xs text-slate-400">HW該当求人数 / 昼間人口</div>
          <div className="text-xl font-bold text-white" data-testid="rd-difficulty-metrics-hw_count">
            {fmt(m.hw_count)} 件
          </div>
          <div className="mt-1 text-xs text-slate-400" data-testid="rd-difficulty-metrics-population">
            人口 {fmt(m.population)}
          </div>
        </div>
        <div className={CARD}>
          <div className="text-xs text-slate-400">全国同条件の何%を占有</div>
          <div
            className="text-2xl font-bold text-white"
            data-testid="rd-difficulty-metrics-area_share_of_national"
          >
            {fmt(m.area_share_of_national * 100, 2)}
            <span className="ml-1 text-sm text-slate-400">%</span>
          </div>
          <div className="mt-1 text-xs text-slate-400" data-testid="rd-difficulty-metrics-national_hw_count">
            全国 {fmt(m.national_hw_count)} 件
          </div>
        </div>
      </div>
      {d.so_what ? (
        <div className={SO_WHAT} data-testid="rd-difficulty-so_what">
          📝 {d.so_what}
        </div>
      ) : null}
    </>,
  );
}

// ---- Panel 2: talent pool ----

export function renderTalentPool(d: RdTalentPoolResponse): PanelView {
  const m = d.metrics;
  const diff = m.commuter_inflow;
  return done(
    '完了',
    <>
      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
        <div className={CARD}>
          <div className="text-xs text-slate-400">昼間人口</div>
          <div className="text-xl font-bold text-white" data-testid="rd-talent_pool-metrics-day_population">
            {fmt(m.day_population)}
          </div>
        </div>
        <div className={CARD}>
          <div className="text-xs text-slate-400">夜間人口</div>
          <div className="text-xl font-bold text-white" data-testid="rd-talent_pool-metrics-night_population">
            {fmt(m.night_population)}
          </div>
        </div>
        <div className={CARD}>
          <div className="text-xs text-slate-400">差分（昼-夜）</div>
          <div
            className={`text-xl font-bold ${diff >= 0 ? 'text-blue-400' : 'text-orange-400'}`}
            data-testid="rd-talent_pool-metrics-commuter_inflow"
          >
            {signPrefix(diff)}
            {fmt(diff)}
          </div>
        </div>
        <div className={CARD}>
          <div className="text-xs text-slate-400">昼夜比</div>
          <div className="text-xl font-bold text-white" data-testid="rd-talent_pool-metrics-day_night_ratio">
            {m.day_night_ratio > 0 ? fmt(m.day_night_ratio, 2) : '—'}
          </div>
        </div>
      </div>
      <div className="text-xs text-slate-500">
        ※ 数値は Agoop 人流データ（平日 昼/深夜 の月平均滞在人口）。
        解釈・採用戦略への示唆は下部の Panel 8「AI 示唆」を参照してください。
      </div>
    </>,
  );
}

// ---- Panel 3: inflow (shown as under development; the old page did not load it either) ----

export function renderInflowPlaceholder(): PanelView {
  return done(
    '開発中',
    <div className="flex items-center justify-center py-10 text-sm text-slate-500">🚧 開発中</div>,
    'muted',
  );
}

// ---- Panel 4: competitors ----

type CompetitorRow = RdCompetitorItem & { index: number };

function featureText(c: RdCompetitorItem): string {
  const parts: string[] = [];
  if (c.sn_industry) parts.push(c.sn_industry);
  if (c.sales_range) parts.push(`売上: ${c.sales_range}`);
  if (c.credit_score) parts.push(`信用 ${fmt(c.credit_score, 1)}`);
  return parts.join(' / ');
}

// DataTable has no per-row attributes, so the "row" testid sits on a span in the first cell.
const COMPETITOR_COLUMNS: DataTableColumn<CompetitorRow>[] = [
  {
    key: 'name',
    header: '企業名',
    render: (c) => {
      const name = c.name || '—';
      return (
        <span data-testid="rd-competitors-row">
          <span data-testid="rd-competitors-name">
            {c.corporate_number ? (
              <a
                href={`/report/company/${encodeURIComponent(c.corporate_number)}`}
                target="_blank"
                rel="noopener"
                className="text-blue-300 hover:text-blue-200 hover:underline"
              >
                {name}
              </a>
            ) : (
              name
            )}
          </span>
        </span>
      );
    },
  },
  {
    key: 'hw_postings_count',
    header: 'HW求人数',
    align: 'right',
    render: (c) => <span data-testid="rd-competitors-hw_postings_count">{fmt(c.hw_postings_count)}</span>,
  },
  {
    key: 'employees',
    header: '従業員数',
    align: 'right',
    render: (c) => <span data-testid="rd-competitors-employees">{fmt(c.employees)}</span>,
  },
  {
    key: 'sales_amount',
    header: '売上 (万円)',
    align: 'right',
    render: (c) => <span data-testid="rd-competitors-sales_amount">{fmt(c.sales_amount)}</span>,
  },
  {
    key: 'features',
    header: '業種・特徴',
    render: (c) => <span data-testid="rd-competitors-features">{featureText(c)}</span>,
  },
];

export function renderCompetitors(d: RdCompetitorsResponse): PanelView {
  const list = asArray(d.companies);
  if (list === null) return errorView('データ形式不正');
  if (list.length === 0) {
    return done(
      '完了（0件）',
      <div className={EMPTY}>
        該当する競合企業がありません（SalesNow に該当業種×エリアの登録なし、または HW未掲載）。
      </div>,
      'muted',
    );
  }
  const rows: CompetitorRow[] = d.companies.map((c, index) => ({ ...c, index }));
  return done(
    `完了（${String(rows.length)}社）`,
    <>
      <DataTable
        columns={COMPETITOR_COLUMNS}
        rows={rows}
        rowKey={(r) => `${r.corporate_number}-${String(r.index)}`}
      />
      <p className="mt-2 text-xs text-slate-500">
        ※ SalesNow 登録企業と HW 求人を名称一致でJOIN。従業員数降順で並べています（ランキング評価ではありません）。企業名クリックで詳細ページを別タブ表示。
      </p>
    </>,
  );
}

// ---- Panel 5: condition gap ----

function GapCell({
  own,
  bench,
  unit,
  digits,
  testId,
}: {
  own: number | null;
  bench: number | null;
  unit: string;
  digits?: number;
  testId: string;
}) {
  if (own === null || bench === null || own === 0 || bench === 0) {
    return (
      <span className="text-slate-500" data-testid={testId}>
        —
      </span>
    );
  }
  const diff = own - bench;
  return (
    <span className={diff >= 0 ? 'text-green-400' : 'text-red-400'} data-testid={testId}>
      {signPrefix(diff)}
      {fmt(diff, digits ?? 0)}
      {unit}
    </span>
  );
}

function GapBox({
  title,
  titleClass,
  panelKey,
  stats,
  own,
}: {
  title: string;
  titleClass: string;
  /** Field path of the benchmark. */
  panelKey: 'industry_median' | 'all_industry_median';
  stats: RdConditionGapResponse['industry_median'];
  own: RdConditionGapResponse['company'];
}) {
  const gapKey = panelKey === 'industry_median' ? 'gap_industry' : 'gap_all';
  const ownSalary = toManYen(own.annual_income_estimated);
  const benchSalary = toManYen(stats.annual_income);
  const t = (field: string): string => `rd-condition_gap-${panelKey}-${field}`;
  const g = (field: string): string => `rd-condition_gap-${gapKey}-${field}`;
  return (
    <div className={CARD}>
      <h4 className={`mb-2 text-sm font-semibold ${titleClass}`}>
        {title}{' '}
        <span className="text-[10px] text-slate-500" data-testid={t('sample_size')}>
          (n={fmt(stats.sample_size)})
        </span>
      </h4>
      <table className="w-full text-xs">
        <tbody>
          <tr>
            <td className="py-1 text-slate-400">年収</td>
            <td className="py-1 text-right text-slate-200" data-testid={t('annual_income')}>
              {fmt(benchSalary)}万
            </td>
            <td className="py-1 text-right">
              <GapCell own={ownSalary} bench={benchSalary} unit="万" testId={g('annual_income')} />
            </td>
          </tr>
          <tr>
            <td className="py-1 text-slate-400">年休</td>
            <td className="py-1 text-right text-slate-200" data-testid={t('annual_holidays')}>
              {fmt(stats.annual_holidays)}日
            </td>
            <td className="py-1 text-right">
              <GapCell
                own={own.annual_holidays}
                bench={stats.annual_holidays}
                unit="日"
                testId={g('annual_holidays')}
              />
            </td>
          </tr>
          <tr>
            <td className="py-1 text-slate-400">賞与月数</td>
            <td className="py-1 text-right text-slate-200" data-testid={t('bonus_months')}>
              {fmt(stats.bonus_months, 1)}ヶ月
            </td>
            <td className="py-1 text-right">
              <GapCell
                own={own.bonus_months}
                bench={stats.bonus_months}
                unit="ヶ月"
                digits={1}
                testId={g('bonus_months')}
              />
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

export function renderConditionGap(d: RdConditionGapResponse): PanelView {
  return done(
    '完了',
    <>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <GapBox
          title="業界内 中央値との差"
          titleClass="text-blue-300"
          panelKey="industry_median"
          stats={d.industry_median}
          own={d.company}
        />
        <GapBox
          title="全業界 中央値との差"
          titleClass="text-purple-300"
          panelKey="all_industry_median"
          stats={d.all_industry_median}
          own={d.company}
        />
      </div>
      {d.interpretation ? (
        <div className={SO_WHAT} data-testid="rd-condition_gap-interpretation">
          📝 {d.interpretation}
        </div>
      ) : null}
    </>,
  );
}

// ---- Panel 6: market trend ----

function TrendChart({ data }: { data: RdMarketTrendResponse }) {
  const option = useMemo(() => trendChartOption(data), [data]);
  return <EChart option={option} testId="rd-chart-trend" height={320} />;
}

export function renderMarketTrend(d: RdMarketTrendResponse): PanelView {
  if (d.months.length === 0 || d.counts.length === 0) {
    return done(
      '完了（データなし）',
      <div className={EMPTY}>HW時系列データが該当範囲で取得できませんでした。</div>,
      'muted',
    );
  }
  const growthPrefix = d.is_sample ? '期間中の変動: ' : '6ヶ月前比: ';
  const growthLabel = `${growthPrefix}${signPrefix(d.growth_rate_pct)}${fmt(d.growth_rate_pct, 1)}%`;
  return done(
    '完了',
    <>
      {d.is_sample ? (
        <div
          className="mb-2 rounded border border-blue-700/60 bg-blue-900/30 p-2 text-xs text-blue-200"
          data-testid="rd-market_trend-is_sample"
        >
          <span className="font-bold">ℹ️ 参考指標</span>:
          このグラフは業界サンプル件数（給与統計用の抽出データ）の推移です。市場全体の総求人数とは異なります。
        </div>
      ) : null}
      <div className="mb-2 text-xs text-slate-400" data-testid="rd-market_trend-growth_rate_pct">
        {growthLabel}
      </div>
      <TrendChart data={d} />
      {d.interpretation ? (
        <div className={SO_WHAT} data-testid="rd-market_trend-interpretation">
          📝 {d.interpretation}
        </div>
      ) : null}
      {d.data_source ? (
        <div className="mt-2 text-[10px] italic text-slate-500" data-testid="rd-market_trend-data_source">
          データ源: {d.data_source}
        </div>
      ) : null}
    </>,
  );
}

// ---- Panel 7: opportunity map ----

function OpportunityChart({ data }: { data: RdOpportunityMapResponse }) {
  const option = useMemo(() => opportunityChartOption(data), [data]);
  return <EChart option={option} testId="rd-chart-opportunity" height={400} />;
}

export function renderOpportunityMap(d: RdOpportunityMapResponse): PanelView {
  const cities = asArray(d.municipalities);
  if (cities === null || cities.length === 0) {
    return done(
      '完了（0件）',
      <div className={EMPTY}>
        該当データなし。都道府県を選択してください。Agoop 人流データと HW求人の両方が必要です。
      </div>,
      'muted',
    );
  }
  return done(
    `完了（${String(cities.length)}件）`,
    <>
      <OpportunityChart data={d} />
      <p className="mt-2 text-xs text-slate-500">
        ※ スコア = HW求人数 ÷ 昼人口 × 1000（人口千人あたり求人数）。値が小さいほど「穴場」、大きいほど「激戦」。相関であり因果ではありません。
      </p>
      {d.note ? (
        <div className="mt-2 text-[10px] italic text-slate-500" data-testid="rd-opportunity_map-note">
          {d.note}
        </div>
      ) : null}
    </>,
  );
}

// ---- Panel 8: insights ----

/** severity_rank: 0 重大 / 1 注意 / 2 情報 / 3 良好 (the API sends Japanese severity names). */
export function severityClass(rank: number): string {
  switch (rank) {
    case 0:
      return 'border-red-500 bg-red-900/20';
    case 1:
      return 'border-orange-500 bg-orange-900/20';
    case 2:
      return 'border-blue-500 bg-blue-900/20';
    case 3:
      return 'border-green-500 bg-green-900/20';
    default:
      return 'border-slate-500 bg-navy-900/40';
  }
}

export function renderInsights(d: RdInsightsResponse): PanelView {
  const list = asArray(d.insights);
  if (list === null) return errorView('データ形式不正');
  if (list.length === 0) {
    return done('完了（0件）', <div className={EMPTY}>該当する示唆がありません。</div>, 'muted');
  }
  return done(
    `完了（${String(list.length)}件）`,
    <>
      <div className="space-y-3">
        {d.insights.map((ins, i) => (
          <div
            key={`${ins.pattern_id}-${String(i)}`}
            className={`rounded border-l-4 p-3 ${severityClass(ins.severity_rank)}`}
            data-testid="rd-insights-row"
            data-severity-rank={ins.severity_rank}
          >
            <div className="flex items-start justify-between gap-2">
              <div className="flex-1">
                <div className="mb-1 text-xs text-slate-400" data-testid="rd-insights-pattern_id">
                  {ins.pattern_id}
                </div>
                <div className="text-sm font-semibold text-white" data-testid="rd-insights-title">
                  {ins.title}
                </div>
                <div className="mt-1 text-xs text-slate-300" data-testid="rd-insights-message">
                  {ins.message}
                </div>
              </div>
            </div>
            {ins.hr_action ? (
              <div className="mt-2 border-t border-slate-700/50 pt-2">
                <div className="text-xs text-slate-400">推奨アクション:</div>
                <div className="mt-0.5 text-xs text-amber-200" data-testid="rd-insights-hr_action">
                  👉 {ins.hr_action}
                </div>
              </div>
            ) : null}
          </div>
        ))}
      </div>
      <p className="mt-3 text-xs text-slate-500">
        ※ 示唆は HW データからの「傾向」に基づきます。因果関係の保証ではありません。
      </p>
    </>,
  );
}

// ---- Panel 9: talent pool expansion ----

function TierBox({
  name,
  label,
  tier,
  color,
}: {
  name: 'tier_30min' | 'tier_60min';
  label: string;
  tier: RdCommuteTier;
  color: string;
}) {
  const t = (field: string): string => `rd-talent_pool_expansion-${name}-${field}`;
  return (
    <div className={CARD}>
      <div className={`mb-1 text-xs font-semibold ${color}`}>
        {label} (上位 <span data-testid={t('municipality_count')}>{tier.municipality_count}</span>{' '}
        市区町村)
      </div>
      <div className="grid grid-cols-2 gap-2 text-sm">
        <div>
          <div className="text-xs text-slate-400">失業者プール</div>
          <div className="text-xl font-bold text-white" data-testid={t('unemployment_pool')}>
            +{fmt(tier.unemployment_pool)} <span className="text-xs text-slate-400">人</span>
          </div>
        </div>
        <div>
          <div className="text-xs text-slate-400">HW 求人</div>
          <div className="text-xl font-bold text-white" data-testid={t('hw_postings')}>
            +{fmt(tier.hw_postings)} <span className="text-xs text-slate-400">件</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function BreakdownTable({ tier, tierId }: { tier: RdCommuteTier; tierId: string }) {
  if (tier.breakdown.length === 0) {
    return <div className="py-2 text-xs text-slate-500">該当データなし</div>;
  }
  const t = (field: string): string => `rd-talent_pool_expansion-breakdown-${field}`;
  return (
    <table className="w-full text-xs">
      <thead>
        <tr className="border-b border-slate-700 text-slate-400">
          <th className="py-1 text-left">市区町村</th>
          <th className="py-1 text-right">通勤者数</th>
          <th className="py-1 text-right">失業者</th>
          <th className="py-1 text-right">HW 求人</th>
        </tr>
      </thead>
      <tbody>
        {tier.breakdown.map((r, i) => (
          <tr
            key={`${r.prefecture}-${r.municipality}-${String(i)}`}
            className="border-b border-slate-800/50"
            data-testid="rd-talent_pool_expansion-row"
            data-tier={tierId}
          >
            <td className="py-1 text-slate-200" data-testid={t('municipality')}>
              {r.prefecture} {r.municipality}
            </td>
            <td className="text-right text-slate-300" data-testid={t('commuters')}>
              {fmt(r.commuters)}
            </td>
            <td className="text-right text-slate-300" data-testid={t('unemployment')}>
              {fmt(r.unemployment)}
            </td>
            <td className="text-right text-slate-300" data-testid={t('hw_postings')}>
              {fmt(r.hw_postings)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function renderTalentPoolExpansion(d: RdTalentPoolExpansionResponse): PanelView {
  if (!d.is_data_available) {
    return done(
      'データなし',
      <div className="rounded bg-slate-800/40 p-3 text-xs text-slate-300">
        通勤 OD データが当該市区町村に対して未投入のため、試算できませんでした。
        （fail-soft）別エリアで再試行してください。
      </div>,
      'muted',
    );
  }
  const t30 = d.tier_30min;
  const t60 = d.tier_60min;
  const notes = d.notes;
  const total = t30.municipality_count + t60.municipality_count;
  const note = (field: string, text: string): ReactNode => (
    <div data-testid={`rd-talent_pool_expansion-notes-${field}`}>※ {text}</div>
  );
  return done(
    `完了（${String(total)} 市区町村）`,
    <>
      <div className="mb-3 text-xs text-slate-400">
        現在地:{' '}
        <span className="font-medium text-slate-200" data-testid="rd-talent_pool_expansion-current">
          {d.current.prefecture} {d.current.municipality}
        </span>
      </div>
      <div className="mb-4 grid grid-cols-1 gap-3 md:grid-cols-2">
        <TierBox name="tier_30min" label="30 分圏" tier={t30} color="text-blue-300" />
        <TierBox name="tier_60min" label="60 分圏 (30 分圏を除く)" tier={t60} color="text-purple-300" />
      </div>
      <details className="mt-2">
        <summary className="cursor-pointer text-xs text-slate-300 hover:text-white">▼ 内訳 (市区町村別)</summary>
        <div className="mt-2 space-y-3">
          <div>
            <div className="mb-1 text-xs text-blue-300">30 分圏</div>
            <BreakdownTable tier={t30} tierId="30min" />
          </div>
          <div>
            <div className="mb-1 text-xs text-purple-300">60 分圏 (30 分圏を除く)</div>
            <BreakdownTable tier={t60} tierId="60min" />
          </div>
        </div>
      </details>
      <div className="mt-3 space-y-0.5 rounded bg-slate-800/40 p-2 text-xs text-slate-400">
        {note('data_source_od', notes.data_source_od || '通勤 OD は国勢調査 2020 年ベース (5 年遅れ)')}
        {note('tier_definition', notes.tier_definition || '30/60 分圏は OD volume 上位の固定件数 (実距離ではない)')}
        {note('caveat_pool', notes.caveat_pool || '失業者プール拡大は通勤可能性であり応募意向ではない')}
        {note('hw_scope', notes.hw_scope || 'HW 求人は HW 掲載のみ')}
      </div>
    </>,
  );
}

/** Page-level scope notes, always shown (the Panel 4/5/6 error bodies carry none of their own). */
export function ScopeNotes() {
  return (
    <div className="space-y-1" data-testid="rd-scope-notes">
      <Note kind="hw-scope" />
      <Note kind="correlation" />
    </div>
  );
}
