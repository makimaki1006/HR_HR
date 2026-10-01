import { Component, useEffect, useRef, useState, type ErrorInfo, type ReactNode } from 'react';
import { ApiAbortedError, AuthRequiredError, apiGet, type ApiResult } from '../../api/client';
import type { GeoMunicipalityOption } from '../../generated/GeoMunicipalityOption';
import type { GeoPrefectureOption } from '../../generated/GeoPrefectureOption';
import { AppShell, useFilters } from '../../shell';
import { redirectToLogin } from '../../shell/navigation';
import { loadAllPanels, PANELS, type PanelName } from './loader';
import { errorView, ScopeNotes, TONE_CLASS, type PanelView, type StatusTone } from './panels';
import {
  EMP_TYPES,
  EMPTY_FORM,
  JOB_TYPES,
  validateForm,
  type DiagnosisForm,
} from './query';

export const GEO_PREFECTURES_PATH = '/api/app/geo/prefectures';
export const geoMunicipalitiesPath = (prefecture: string): string =>
  `/api/app/geo/municipalities?prefecture=${encodeURIComponent(prefecture)}`;

type PanelSlot = { status: 'idle' } | { status: 'loading' } | { status: 'view'; view: PanelView };

type PanelSlots = Record<PanelName, PanelSlot>;

const initialSlots = (status: 'idle' | 'loading'): PanelSlots =>
  Object.fromEntries(PANELS.map((p) => [p.name, { status }])) as PanelSlots;

const INPUT = 'rounded border border-gray-600 bg-gray-700 px-2 py-1.5 text-sm text-white';
const CARD = 'rounded-lg border border-slate-700 bg-navy-800 p-4';

function PanelShell({
  name,
  title,
  dataStatus,
  statusText,
  tone,
  children,
}: {
  name: PanelName;
  title: string;
  dataStatus: 'idle' | 'loading' | 'error' | 'done';
  statusText: string;
  tone: StatusTone;
  children: ReactNode;
}) {
  return (
    <section className={CARD} data-testid={`rd-panel-${name}`} data-status={dataStatus}>
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-lg font-semibold text-white">{title}</h3>
        <span className={`text-xs ${TONE_CLASS[tone]}`} data-testid={`rd-panel-${name}-status`}>
          {statusText}
        </span>
      </div>
      {name === 'condition_gap' ? (
        <div className="mb-3 border-l-2 border-amber-500 bg-amber-900/30 px-3 py-1.5 text-xs text-amber-200">
          ⚠️ HW 求人は市場実勢より給与を低めに設定する慣習あり。HW
          内相対位置の診断のため、絶対的競争力の保証ではありません。
          <a href="/tab/guide" className="ml-1 text-amber-100 underline">
            詳細
          </a>
        </div>
      ) : null}
      {children}
    </section>
  );
}

/** A render exception in one panel body becomes that panel's error state; the others stay. */
class PanelBoundary extends Component<
  { name: PanelName; title: string; children: ReactNode },
  { error: Error | null }
> {
  override state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error): { error: Error } {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[RD]', this.props.name, 'render failed:', error, info.componentStack);
  }

  override render(): ReactNode {
    if (this.state.error === null) return this.props.children;
    const view = errorView(this.state.error.message);
    return (
      <PanelShell
        name={this.props.name}
        title={this.props.title}
        dataStatus="error"
        statusText={view.statusText}
        tone={view.tone}
      >
        {view.body}
      </PanelShell>
    );
  }
}

function PanelSlotView({ name, title, slot }: { name: PanelName; title: string; slot: PanelSlot }) {
  if (slot.status === 'idle') {
    return (
      <PanelShell name={name} title={title} dataStatus="idle" statusText="待機中" tone="muted">
        <div className="py-6 text-center text-sm text-slate-500">診断実行待ち</div>
      </PanelShell>
    );
  }
  if (slot.status === 'loading') {
    return (
      <PanelShell name={name} title={title} dataStatus="loading" statusText="取得中..." tone="loading">
        <div className="flex items-center justify-center py-8 text-sm text-slate-400">
          <div className="animate-pulse">⏳ データ取得中...</div>
        </div>
      </PanelShell>
    );
  }
  const v = slot.view;
  return (
    <PanelShell name={name} title={title} dataStatus={v.status} statusText={v.statusText} tone={v.tone}>
      {v.body}
    </PanelShell>
  );
}

type GeoState<T> = { status: 'idle' | 'loading' | 'error' } | { status: 'ok'; list: T[] };

/** Municipality list of one prefecture (the answer is only valid for that prefecture). */
interface CityAnswer {
  prefecture: string;
  result: GeoState<GeoMunicipalityOption>;
}

export function RecruitmentDiagScreen() {
  // filters: header bar (prefecture / municipality) on, children wait for the first sync.
  return (
    <AppShell screen="recruitment-diag" filters>
      <RecruitmentDiag />
    </AppShell>
  );
}

/** A geo response: sign-in expiry goes to the login page, an abort means "nobody is waiting". */
function geoOutcome<T>(r: ApiResult<T[]>): GeoState<T> | null {
  if (r.ok) return { status: 'ok', list: r.data };
  if (r.error instanceof ApiAbortedError) return null;
  if (r.error instanceof AuthRequiredError) {
    redirectToLogin();
    return null;
  }
  return { status: 'error' };
}

/**
 * The session filters (useFilters, shared with the header bar) are the source of the prefecture
 * and municipality: this screen only derives names -> prefcode / citycode from the geo lists and
 * writes changes back with setPrefecture / setMunicipality (session + URL). The industry filter
 * (job_types / industry_raws) is a different thing from this screen's 業種 and is not touched.
 */
export function RecruitmentDiag() {
  const { filters, ready, syncing, setPrefecture, setMunicipality } = useFilters();
  const [fields, setFields] = useState<DiagnosisForm>(EMPTY_FORM);
  const [prefs, setPrefs] = useState<GeoState<GeoPrefectureOption>>({ status: 'loading' });
  const [cityAnswer, setCityAnswer] = useState<CityAnswer | null>(null);
  const [slots, setSlots] = useState<PanelSlots>(() => initialSlots('idle'));
  const [started, setStarted] = useState(false);
  const [running, setRunning] = useState(false);
  const [finished, setFinished] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [runId, setRunId] = useState(0);
  const runController = useRef<AbortController | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    void apiGet<GeoPrefectureOption[]>(GEO_PREFECTURES_PATH, { signal: controller.signal }).then((r) => {
      if (controller.signal.aborted) return;
      const next = geoOutcome(r);
      if (next !== null) setPrefs(next);
    });
    return () => {
      controller.abort();
    };
  }, []);

  // One request per prefecture. The cleanup aborts a superseded request, and the `aborted` check
  // keeps a response that was already on its way (apiGet does not re-check after the body is
  // read) from replacing the list of the newer prefecture.
  useEffect(() => {
    if (!ready || filters.prefecture === '') return;
    const prefecture = filters.prefecture;
    const controller = new AbortController();
    void apiGet<GeoMunicipalityOption[]>(geoMunicipalitiesPath(prefecture), { signal: controller.signal }).then((r) => {
      if (controller.signal.aborted) return;
      const result = geoOutcome(r);
      if (result !== null) setCityAnswer({ prefecture, result });
    });
    return () => {
      controller.abort();
    };
  }, [ready, filters.prefecture]);

  useEffect(
    () => () => {
      runController.current?.abort();
    },
    [],
  );

  // Derived, never stored: a list that belongs to another prefecture counts as "loading".
  const cities: GeoState<GeoMunicipalityOption> =
    filters.prefecture === ''
      ? { status: 'idle' }
      : cityAnswer !== null && cityAnswer.prefecture === filters.prefecture
        ? cityAnswer.result
        : { status: 'loading' };

  const prefcode =
    prefs.status === 'ok' ? (prefs.list.find((p) => p.name === filters.prefecture)?.prefcode ?? null) : null;
  const citycode =
    cities.status === 'ok' && filters.municipality !== ''
      ? (cities.list.find((c) => c.name === filters.municipality)?.citycode ?? null)
      : null;
  /** What the panels are asked with: the session's prefecture / municipality, nothing else. */
  const form: DiagnosisForm = {
    ...fields,
    prefecture: filters.prefecture,
    prefcode,
    municipality: filters.municipality,
    citycode,
  };

  // A session value the lists do not know is still shown, so what is displayed is what is sent.
  const prefUnlisted =
    form.prefecture !== '' &&
    !(prefs.status === 'ok' && prefs.list.some((p) => p.name === form.prefecture));
  const cityUnlisted =
    cities.status === 'ok' &&
    form.municipality !== '' &&
    !cities.list.some((c) => c.name === form.municipality);

  // No run while a run is in flight, a filter save is pending (the API falls back to the session
  // for what it does not get), or the municipality list that gives the citycode is loading.
  const runBlocked = running || syncing || cities.status === 'loading';

  const set = (patch: Partial<DiagnosisForm>): void => {
    setFields((f) => ({ ...f, ...patch }));
  };

  const onPrefChange = (name: string): void => {
    void setPrefecture(name);
  };

  const onCityChange = (name: string): void => {
    void setMunicipality(name);
  };

  const run = (): void => {
    const problem = validateForm(form);
    setMessage(problem);
    if (problem !== null) return;

    runController.current?.abort();
    const controller = new AbortController();
    runController.current = controller;
    setRunId((n) => n + 1);
    setStarted(true);
    setRunning(true);
    setFinished(false);
    setSlots(initialSlots('loading'));
    void loadAllPanels(form, controller.signal, (name, view) => {
      // A view that settles after this run was replaced or the screen closed is not painted.
      if (controller.signal.aborted) return;
      setSlots((prev) => ({ ...prev, [name]: { status: 'view', view } }));
    }).then(() => {
      if (controller.signal.aborted) return;
      setRunning(false);
      setFinished(true);
    });
  };

  return (
    <div className="space-y-6" id="rd-root">
      <header className={CARD}>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h2 className="text-xl font-bold text-white">
              📊 採用診断 <span className="text-base font-normal text-blue-400">人事向け統合レポート</span>
            </h2>
            <p className="mt-1 text-xs text-slate-400">
              業種・エリア・雇用形態を選択すると、採用難度／人材プール／競合／市場動向を一括診断します。
              すべて HW 求人データに基づく相対分析です。
            </p>
          </div>
          <div className="text-right text-xs text-slate-500">
            <div>データ範囲: HW掲載求人のみ</div>
            <div>更新: 直近スナップショット</div>
          </div>
        </div>
      </header>

      <ScopeNotes />

      <section className={CARD} aria-labelledby="rd-form-title">
        <h3 id="rd-form-title" className="mb-3 text-sm font-semibold text-slate-200">
          🎯 診断条件
        </h3>
        <div className="flex flex-wrap items-end gap-3">
          <div>
            <label htmlFor="rd-job-type" className="mb-1 block text-xs text-gray-400">
              業種
            </label>
            <select
              id="rd-job-type"
              data-testid="rd-form-job-type"
              className={`${INPUT} min-w-[160px]`}
              value={form.jobType}
              onChange={(e) => {
                set({ jobType: e.target.value });
              }}
            >
              <option value="">選択してください</option>
              {JOB_TYPES.map((j) => (
                <option key={j} value={j}>
                  {j}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="rd-emp-type" className="mb-1 block text-xs text-gray-400">
              雇用形態
            </label>
            <select
              id="rd-emp-type"
              data-testid="rd-form-emp-type"
              className={INPUT}
              value={form.empType}
              onChange={(e) => {
                set({ empType: e.target.value });
              }}
            >
              {EMP_TYPES.map((j) => (
                <option key={j} value={j}>
                  {j}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label htmlFor="rd-pref" className="mb-1 block text-xs text-gray-400">
              都道府県
            </label>
            <select
              id="rd-pref"
              data-testid="rd-form-pref"
              className={`${INPUT} min-w-[140px]`}
              value={form.prefecture}
              onChange={(e) => {
                onPrefChange(e.target.value);
              }}
            >
              <option value="">{prefs.status === 'error' ? '取得失敗' : '選択してください'}</option>
              {prefs.status === 'ok'
                ? prefs.list.map((p) => (
                    <option key={p.name} value={p.name}>
                      {p.name}
                    </option>
                  ))
                : null}
              {prefUnlisted ? <option value={form.prefecture}>{form.prefecture}</option> : null}
            </select>
          </div>
          <div>
            <label htmlFor="rd-city" className="mb-1 block text-xs text-gray-400">
              市区町村
            </label>
            <select
              id="rd-city"
              data-testid="rd-form-city"
              className={`${INPUT} min-w-[140px]`}
              value={form.municipality}
              onChange={(e) => {
                onCityChange(e.target.value);
              }}
            >
              {cities.status === 'idle' ? <option value="">-- 都道府県を先に選択 --</option> : null}
              {cities.status === 'loading' ? <option value="">-- 取得中... --</option> : null}
              {cities.status === 'error' ? <option value="">取得失敗</option> : null}
              {cities.status === 'ok' ? (
                <>
                  <option value="">すべて（都道府県全体）</option>
                  {cities.list.map((c, i) => (
                    <option key={`${c.name}-${String(i)}`} value={c.name}>
                      {c.name}
                    </option>
                  ))}
                  {cityUnlisted ? <option value={form.municipality}>{form.municipality}</option> : null}
                </>
              ) : null}
            </select>
          </div>
          <button
            type="button"
            data-testid="rd-run"
            disabled={runBlocked}
            onClick={run}
            className={`rounded bg-blue-600 px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-blue-500 ${runBlocked ? 'opacity-60' : ''}`}
          >
            🔍 診断実行
          </button>
          <span className="ml-2 text-xs text-slate-400" data-testid="rd-global-status">
            {running ? '診断中...' : finished ? '診断完了' : ''}
          </span>
        </div>
        {message === null ? null : (
          <p role="alert" className="mt-3 text-sm text-red-300" data-testid="rd-form-message">
            {message}
          </p>
        )}

        <details className="mt-3 border-t border-slate-700 pt-3">
          <summary className="cursor-pointer select-none text-xs text-slate-300 hover:text-white">
            ⚙️ 自社条件を入力（Panel 5 条件ギャップ診断で使用）
          </summary>
          <div className="mt-3 flex flex-wrap gap-3">
            <div>
              <label htmlFor="rd-own-salary" className="mb-1 block text-xs text-gray-400">
                月給下限（万円）
              </label>
              <input
                type="number"
                id="rd-own-salary"
                data-testid="rd-form-own-salary"
                placeholder="例: 25"
                min="0"
                step="1"
                className={`${INPUT} w-28`}
                value={form.ownSalaryMan}
                onChange={(e) => {
                  set({ ownSalaryMan: e.target.value });
                }}
              />
            </div>
            <div>
              <label htmlFor="rd-own-holidays" className="mb-1 block text-xs text-gray-400">
                年間休日数
              </label>
              <input
                type="number"
                id="rd-own-holidays"
                data-testid="rd-form-own-holidays"
                placeholder="例: 120"
                min="0"
                max="365"
                step="1"
                className={`${INPUT} w-28`}
                value={form.ownHolidays}
                onChange={(e) => {
                  set({ ownHolidays: e.target.value });
                }}
              />
            </div>
            <div>
              <label htmlFor="rd-own-bonus" className="mb-1 block text-xs text-gray-400">
                賞与（月数）
              </label>
              <input
                type="number"
                id="rd-own-bonus"
                data-testid="rd-form-own-bonus"
                placeholder="例: 2.0"
                min="0"
                max="12"
                step="0.1"
                className={`${INPUT} w-28`}
                value={form.ownBonus}
                onChange={(e) => {
                  set({ ownBonus: e.target.value });
                }}
              />
            </div>
          </div>
        </details>
      </section>

      {started ? null : (
        <div className={`${CARD} py-10 text-center text-slate-400`} data-testid="rd-initial-msg">
          <p className="text-sm">上のフォームで業種・エリアを選択して「🔍 診断実行」を押してください。</p>
          <p className="mt-2 text-xs text-slate-500">
            本診断は HW 掲載求人に限定された相対分析です（求人市場全体ではありません）。
          </p>
        </div>
      )}

      <div className={`space-y-6 ${started ? '' : 'hidden'}`} data-testid="rd-results">
        {PANELS.map((p) => (
          <PanelBoundary key={`${p.name}-${String(runId)}`} name={p.name} title={p.title}>
            <PanelSlotView name={p.name} title={p.title} slot={slots[p.name]} />
          </PanelBoundary>
        ))}
        <footer className="border-t border-slate-700 pt-3 text-xs text-slate-500">
          <p>※ 本診断は 公的求人データに基づく相対分析です。民間求人媒体・非公開求人は含まれません。</p>
          <p>
            ※
            相関関係と因果関係は別物です。「傾向がある」「可能性がある」という表現に留めています。絶対的な保証ではありません。
          </p>
        </footer>
      </div>
    </div>
  );
}
