// 営業KPI (React 版、W2)。旧画面 /sales-kpi と同じ JSON (`GET /api/sales-kpi/data`) を
// 1 本読んで描く。状態 (絞り込み・開いている一覧・期間・タブ) はここで持ち、
// 見た目は SalesKpiView に渡す。Shell 非依存 (ヘッダー・戻るリンクも画面の中)。
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ApiAbortedError, apiGet } from '../../api/client';
import {
  CLOSED_CARD_PANEL,
  DEFAULT_CALL_PERIOD,
  toggleCardState,
  type CardPanelState,
  type OpenKey,
  type Scope,
  type SnapMode,
  type TabKey,
} from './calc';
import { SalesKpiView, type UiActions, type UiState } from './SalesKpiView';
import type { CallPeriodKey, SalesKpiData } from './types';

export const DATA_PATH = '/api/sales-kpi/data';
/** 旧画面と同じキー。旧画面で選んだ明暗・チェックがそのまま引き継がれる。 */
export const THEME_KEY = 'salesKpi.theme.v1';
export const HIDE_KEY = 'salesKpi.hidden.v1';

export type LoadState =
  | { status: 'loading' }
  | { status: 'ok'; data: SalesKpiData }
  | { status: 'error'; message: string };

/** `?refresh=1` が付いていればキャッシュを捨てて読み直す (旧画面と同じ)。 */
export function dataPath(search: string): string {
  return DATA_PATH + (search.includes('refresh=1') ? '?refresh=1' : '');
}

function readHidden(): Set<string> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(HIDE_KEY) ?? '[]');
    if (Array.isArray(raw)) return new Set(raw.filter((x): x is string => typeof x === 'string'));
  } catch {
    // プライベートモード等で localStorage が使えなくても画面は動く (保存されないだけ)
  }
  return new Set();
}

function saveHidden(hidden: ReadonlySet<string>): void {
  try {
    localStorage.setItem(HIDE_KEY, JSON.stringify([...hidden]));
  } catch {
    // 保存できなくても画面は動く
  }
}

/** 前に「暗い画面」を選んだ人だけ暗くする。既定は明るい。 */
export function applySavedTheme(): void {
  try {
    if (localStorage.getItem(THEME_KEY) === 'dark') {
      document.documentElement.setAttribute('data-theme', 'dark');
    }
  } catch {
    // 読めなければ明るいまま
  }
}

function toggleTheme(): void {
  const dark = document.documentElement.getAttribute('data-theme') === 'dark';
  const next = dark ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', next);
  try {
    localStorage.setItem(THEME_KEY, next);
  } catch {
    // 保存できなくても切り替えは効く
  }
}

export function LoadStateView({ state }: { state: LoadState }) {
  if (state.status === 'loading') {
    return (
      <div className="loadstate" id="loadstate">
        データを読み込んでいます…
      </div>
    );
  }
  if (state.status === 'error') {
    return (
      <div className="loadstate err" id="loadstate" role="alert">
        <b>データを読み込めませんでした。</b>
        {state.message}
        <br />
        <span className="hint">
          スプレッドシートの KPI営業_ シートが揃っているか、GAS の sales_kpi_sync が動いているかを確認してください。
        </span>
      </div>
    );
  }
  return null;
}

const INITIAL_SCOPE: Scope = { team: 'すべて', person: null, hidden: new Set() };

export function SalesKpiScreen() {
  const [state, setState] = useState<LoadState>({ status: 'loading' });
  // hidden は localStorage から (読めなければ空)。描画前に 1 回だけ読む。
  const [scope, setScope] = useState<Scope>(() => ({ ...INITIAL_SCOPE, hidden: readHidden() }));
  const [openKey, setOpenKey] = useState<OpenKey | null>(null);
  const [card, setCard] = useState<CardPanelState>(CLOSED_CARD_PANEL);
  const [dayKey, setDayKey] = useState<string | null>(null);
  const [weekOpen, setWeekOpen] = useState(false);
  const [callPeriod, setCallPeriod] = useState<CallPeriodKey>(DEFAULT_CALL_PERIOD);
  const [snapMode, setSnapMode] = useState<SnapMode>('week');
  const [tab, setTab] = useState<TabKey>('kpi');
  const [pickOpen, setPickOpen] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    void apiGet<SalesKpiData>(dataPath(window.location.search), { signal: controller.signal }).then(
      (result) => {
        if (result.ok) {
          setState({ status: 'ok', data: result.data });
        } else if (!(result.error instanceof ApiAbortedError)) {
          setState({ status: 'error', message: result.error.message });
        }
      },
    );
    return () => {
      controller.abort();
    };
  }, []);

  const updateHidden = useCallback((mutate: (h: Set<string>) => void) => {
    setScope((s) => {
      const h = new Set(s.hidden);
      mutate(h);
      saveHidden(h);
      // チェックを外した人が個人で選ばれていたら、個人指定を解く (旧画面と同じ)
      const person = s.person !== null && h.has(s.person) ? null : s.person;
      return { ...s, hidden: h, person };
    });
  }, []);

  const actions: UiActions = useMemo(
    () => ({
      setTeam: (team) => {
        setScope((s) => ({ ...s, team, person: null }));
        setOpenKey(null);
        setCard((c) => ({ ...c, openCard: null }));
      },
      setPerson: (id) => {
        setScope((s) => ({ ...s, person: id }));
        setOpenKey(null);
        setCard((c) => ({ ...c, openCard: null }));
      },
      setHidden: (ids, on) => {
        updateHidden((h) => {
          for (const id of ids) {
            if (on) h.add(id);
            else h.delete(id);
          }
        });
      },
      resetHidden: () => {
        updateHidden((h) => {
          h.clear();
        });
      },
      toggleOpen: (key) => {
        setOpenKey((k) => (k === key ? null : key));
        setCard((c) => ({ ...c, openCard: null }));
        setDayKey(null);
        setWeekOpen(false);
        setTimeout(() => {
          document.getElementById('panel')?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }, 50);
      },
      closePanel: () => {
        setOpenKey(null);
      },
      // 今月の成績カードの内訳 (旧 toggleCard)。開閉で掘り下げ・区分・BPO を捨て、下の一覧は閉じる
      toggleCard: (key) => {
        setCard((c) => toggleCardState(c, key));
        setOpenKey(null);
        setDayKey(null);
        setWeekOpen(false);
        setTimeout(() => {
          document.getElementById('panel1')?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }, 50);
      },
      closeCard: () => {
        setCard((c) => ({ ...c, openCard: null }));
      },
      toggleBpoOnly: () => {
        setCard((c) => ({ ...c, bpoOnly: !c.bpoOnly, cardTeam: null, cardPerson: null }));
      },
      setCardSeg: (k) => {
        setCard((c) => ({ ...c, cardSeg: k === null || c.cardSeg === k ? null : k, cardTeam: null, cardPerson: null }));
      },
      toggleCardNt: (name) => {
        setCard((c) => ({ ...c, cardNt: c.cardNt === name ? null : name }));
      },
      setCardTeam: (t) => {
        setCard((c) => ({ ...c, cardTeam: t }));
      },
      setCardPerson: (id) => {
        setCard((c) => ({ ...c, cardPerson: id }));
      },
      setDayKey: (dt) => {
        setDayKey(dt);
        setWeekOpen(false);
      },
      setWeekOpen: (on) => {
        setWeekOpen(on);
        if (on) setDayKey(null);
      },
      setCallPeriod,
      setSnapMode,
      setTab,
      togglePick: () => {
        setPickOpen((p) => !p);
      },
      toggleTheme,
    }),
    [updateHidden],
  );

  if (state.status !== 'ok') {
    return (
      <div className="wrap">
        <LoadStateView state={state} />
      </div>
    );
  }
  const ui: UiState = { scope, openKey, card, dayKey, weekOpen, callPeriod, snapMode, tab, pickOpen };
  return <SalesKpiView data={state.data} ui={ui} actions={actions} />;
}
