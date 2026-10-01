import { ApiAbortedError, apiGet, type ApiResult } from '../../api/client';
import type { RdCompetitorsResponse } from '../../generated/RdCompetitorsResponse';
import type { RdConditionGapResponse } from '../../generated/RdConditionGapResponse';
import type { RdDifficultyResponse } from '../../generated/RdDifficultyResponse';
import type { RdInsightsResponse } from '../../generated/RdInsightsResponse';
import type { RdMarketTrendResponse } from '../../generated/RdMarketTrendResponse';
import type { RdOpportunityMapResponse } from '../../generated/RdOpportunityMapResponse';
import type { RdTalentPoolExpansionResponse } from '../../generated/RdTalentPoolExpansionResponse';
import type { RdTalentPoolResponse } from '../../generated/RdTalentPoolResponse';
import {
  buildCommonQuery,
  buildCompetitorsQuery,
  buildGapQuery,
  type DiagnosisForm,
} from './query';
import {
  errorView,
  renderCompetitors,
  renderConditionGap,
  renderDifficulty,
  renderInflowPlaceholder,
  renderInsights,
  renderMarketTrend,
  renderOpportunityMap,
  renderTalentPool,
  renderTalentPoolExpansion,
  type PanelView,
} from './panels';

export type PanelName =
  | 'difficulty'
  | 'talent_pool'
  | 'inflow'
  | 'competitors'
  | 'condition_gap'
  | 'market_trend'
  | 'opportunity_map'
  | 'insights'
  | 'talent_pool_expansion';

export const API_BASE = '/api/recruitment_diag';

/** Longer than the client default (15 s): the old page had no timeout at all. */
export const PANEL_TIMEOUT_MS = 60_000;

export interface PanelDef {
  name: PanelName;
  title: string;
  /** Null: nothing is fetched (Panel 3 is shown as "under development"). */
  load: ((form: DiagnosisForm, signal: AbortSignal) => Promise<PanelView | null>) | null;
}

/** Request URL and options for one panel. */
function req(path: string, query: string, signal: AbortSignal): [string, { signal: AbortSignal; timeoutMs: number }] {
  return [`${API_BASE}/${path}?${query}`, { signal, timeoutMs: PANEL_TIMEOUT_MS }];
}

/**
 * Turns one panel's result into a view; every failure becomes that panel's error view
 * (abort -> null, i.e. nothing to show).
 */
function toView<T>(result: ApiResult<T>, render: (data: T) => PanelView): PanelView | null {
  if (!result.ok) {
    if (result.error instanceof ApiAbortedError) return null;
    // HTTP failure -> "HTTP 500"; HTTP 200 + {"error": ...} -> the error text (ApiDataError).
    return errorView(result.error.message);
  }
  if (typeof result.data !== 'object' || result.data === null) return errorView('データ形式不正');
  try {
    return render(result.data);
  } catch (e) {
    return errorView(e instanceof Error ? e.message : String(e));
  }
}

/** A thrown error (not expected from apiGet) must not escape and stop the other panels. */
function thrown(e: unknown): PanelView {
  return errorView(e instanceof Error ? e.message : String(e));
}

export const PANELS: readonly PanelDef[] = [
  {
    name: 'difficulty',
    title: '🎯 Panel 1: 採用難度スコア',
    load: (f, s) =>
      apiGet<RdDifficultyResponse>(...req('difficulty', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderDifficulty),
        thrown,
      ),
  },
  {
    name: 'talent_pool',
    title: '👥 Panel 2: 人材プール診断',
    load: (f, s) =>
      apiGet<RdTalentPoolResponse>(...req('talent_pool', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderTalentPool),
        thrown,
      ),
  },
  { name: 'inflow', title: '🔄 Panel 3: 流入元分析', load: null },
  {
    name: 'competitors',
    title: '🏢 Panel 4: 競合企業ランキング',
    load: (f, s) =>
      apiGet<RdCompetitorsResponse>(...req('competitors', buildCompetitorsQuery(f), s)).then(
        (r) => toView(r, renderCompetitors),
        thrown,
      ),
  },
  {
    name: 'condition_gap',
    title: '💰 Panel 5: 条件ギャップ診断',
    load: (f, s) =>
      apiGet<RdConditionGapResponse>(...req('condition_gap', buildGapQuery(f), s)).then(
        (r) => toView(r, renderConditionGap),
        thrown,
      ),
  },
  {
    name: 'market_trend',
    title: '📈 Panel 6: 市場動向',
    load: (f, s) =>
      apiGet<RdMarketTrendResponse>(...req('market_trend', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderMarketTrend),
        thrown,
      ),
  },
  {
    name: 'opportunity_map',
    title: '🗺️ Panel 7: 穴場マップ（市区町村）',
    load: (f, s) =>
      apiGet<RdOpportunityMapResponse>(...req('opportunity_map', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderOpportunityMap),
        thrown,
      ),
  },
  {
    name: 'insights',
    title: '💡 Panel 8: AI 示唆（So What / Next Action）',
    load: (f, s) =>
      apiGet<RdInsightsResponse>(...req('insights', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderInsights),
        thrown,
      ),
  },
  {
    name: 'talent_pool_expansion',
    title: '🚃 Panel 9: 通勤圏人材プール試算',
    load: (f, s) =>
      apiGet<RdTalentPoolExpansionResponse>(...req('talent_pool_expansion', buildCommonQuery(f), s)).then(
        (r) => toView(r, renderTalentPoolExpansion),
        thrown,
      ),
  },
];

/**
 * Loads every panel independently. `onView` is called per panel as soon as it settles; a failure
 * (HTTP error, {"error"}, network error, render exception) only affects that panel.
 * Resolves when all panels have settled.
 */
export async function loadAllPanels(
  form: DiagnosisForm,
  signal: AbortSignal,
  onView: (name: PanelName, view: PanelView) => void,
): Promise<void> {
  await Promise.allSettled(
    PANELS.map(async (def) => {
      const view = def.load === null ? renderInflowPlaceholder() : await def.load(form, signal);
      if (view !== null) onView(def.name, view);
    }),
  );
}
