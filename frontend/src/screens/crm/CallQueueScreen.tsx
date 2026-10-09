import { memo, useCallback, useEffect, useMemo, useReducer, useRef, useState, useSyncExternalStore } from 'react';
import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { CallQueuePartial } from '../../generated/CallQueuePartial';
import { OwnerFilter } from './OwnerFilter';
import { ownerNameMap } from './ownerModel';
import { formatPhoneForDisplay } from './phone';
import {
  DEFAULT_FILTERS, QUEUE_SORTS, QUEUE_TOTAL_NOTE, QUEUE_TOTAL_NOTE_SHORT, dateValue, filtersKey, parseFilters, parseMode,
  queueCountText, screenSearch, withPipeline,
} from './queueModel';
import type { QueueFilters, QueueMode, QueueSort } from './queueModel';
import { DEFAULT_PIPELINE_ID, findPipeline, pipelineName, pipelinesFor, stageName } from './queuePipelines';
import type { QueuePipelineDef } from './queuePipelines';
import { StageFilter } from './StageFilter';
import { useQueuePipelines } from './useQueuePipelines';
import type { PipelinesFetch } from './useQueuePipelines';
import { useCallQueue } from './useCallQueue';
import { useAutoLoadMore } from './useAutoLoadMore';
import { ActivityLog, DealLinks, DealOverview, rawStopLabel } from './DealDetail';
import { ConflictDialog } from './WriteWidgets';
import { fakeWriteApi } from './fakeWrite';
import { liveWriteApi } from './crmWrite';
import type { WriteApi } from './crmWrite';
import { useCrmWrite } from './useCrmWrite';
import { useWriteBindings } from './writeBindings';
import type { FieldDef } from './writeModel';
import type { CallBarInfo, StopLabel } from './DealDetail';
import { ZoomPhonePanel } from './ZoomPhonePanel';
import { fixtureDetailFetch, liveDetailFetch, useDealDetail } from './useDealDetail';
import type { DetailFetch } from './useDealDetail';
import { useZoomPhone } from './useZoomPhone';
import type { ZoomOptions } from './useZoomPhone';
import type { QueueFetch } from './useCallQueue';
import { fixtureOwnersFetch, liveOwnersFetch, useOwners } from './useOwners';
import type { OwnersFetch } from './useOwners';
import { CallResultForm } from './CallResultForm';
import { CenterPanel, CenterTabBar, LinkOpenerContext, LinkView, searchTab, useCenterTabs } from './CenterTabs';
import { DEAL_TAB, SEARCH_TAB, dealJobSearchUrl } from './centerLinks';
import { Dock } from './Dock';
import { clearLegacyLayout, columnOf, dockReducer, loadLayoutWithNotice, localStorageOrNull, saveLayout } from './dockModel';
import type { DockAction, PanelId } from './dockModel';
import { PropertyPanel } from './PropertyPanel';
import { loadSelected, sanitizeSelected, saveSelected } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import { usePropertyCatalog } from './usePropertyCatalog';
import type { CatalogFetch } from './usePropertyCatalog';
import { PARTIAL_LABELS } from './workspaceModel';
import {
  clearDraftEntry, draftKey, editDraft, emptyResultDraft, emptyStore, loadStore, markRecorded, msUntilNextJstMidnight, nextUnrecorded, optionLabel, saveStore,
  sessionStorageOrNull, todayJst, validateResultDraft,
} from './callResultModel';
import type { DraftStore, ResultDraft } from './callResultModel';
import { useResultDefinitions } from './useResultDefinitions';
import type { MetadataFetch } from './useResultDefinitions';
import { useCurrentUser } from './useCurrentUser';
import type { UserFetch } from './useCurrentUser';
import type { DialResult, ZoomPhone } from './useZoomPhone';
import type { EmbedPhase } from './useZoomPhone';
import { toE164Jp } from './smartEmbed';
import type { CallState } from './smartEmbed';
import './crm.css';
import './queue.css';

/** パネルの中で、架電先を選ぶ前・読み込み中・失敗のときに出す一言 (詳しい案内は「案件の概要」に出す) */
export function panelPlaceholder(selected: boolean, phase: 'idle' | 'loading' | 'ready' | 'error' | 'forbidden' | 'waiting'): string {
  if (!selected || phase === 'idle') return '架電一覧から架電先を選ぶと、ここに表示します。';
  if (phase === 'loading' || phase === 'waiting') return '案件の情報を読み込み中…';
  return '案件の情報を表示できません。「案件の概要」の案内を確認してください。';
}

/** 案件の画面から発信した記録。callId は発信の後に最初に始まった (番号の合う) 通話のもの */
export interface DialedFor {
  dealId: string;
  mode: QueueMode;
  /** 発信した番号 (+81…) */
  number: string | null;
  /** 発信した時点で Zoom が持っていた通話 (前の通話の「終了」を新しい案件に付けない) */
  staleCallId: string | null;
  callId: string | null;
}

/** 発信の後に始まった通話を、その発信のものとみなせるか (別の通話・着信・番号違いは紐づけない) */
export function bindsToDial(d: DialedFor, call: CallState): boolean {
  if (d.callId !== null || call.callId === null || call.callId === d.staleCallId || call.phase === 'idle') return false;
  if (call.direction !== null && call.direction !== 'outbound') return false;
  const n = toE164Jp(call.number);
  return d.number === null || n === null || n === d.number;
}

const PHONE_SOURCE_LABELS: Record<string, string> = { deal: '案件', contact: '担当者', mobile: '担当者(携帯)', company: '会社' };

/** 画面の高さがこれ以下 (列が低い) なら、「案件の概要」を 1 行で出す (利用者が切り替えるまで) */
export const COMPACT_OVERVIEW_QUERY = '(max-height: 800px)';

/** メディアクエリに合っているか (matchMedia の無い環境では false) */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback((cb: () => void) => {
    if (typeof window.matchMedia !== 'function') return () => undefined;
    const m = window.matchMedia(query);
    m.addEventListener('change', cb);
    return () => { m.removeEventListener('change', cb); };
  }, [query]);
  return useSyncExternalStore(subscribe, () => typeof window.matchMedia === 'function' && window.matchMedia(query).matches, () => false);
}

/** 配置を新しい既定に切り替えたときの 1 回だけの案内 */
export const LAYOUT_UPDATED_NOTE = '画面の配置を更新しました。「求人検索・リンク先」を右の列に移し、検索結果を広く表示します(パネルはタブのドラッグか「⋮」で移せます)。';

/** 矢印キーで行を移ったとき、詳細の取得を待つ時間 (押しっぱなしで HubSpot を連続で呼ばない) */
export const KEY_SELECT_DELAY_MS = 300;

const ymd = (raw: string | null) => dateValue(raw)?.replaceAll('-', '/') ?? null;
/** 一覧の行は幅が狭いので月日だけ (年は title に残す) */
const md = (raw: string | null) => dateValue(raw)?.slice(5).replace('-', '/') ?? null;

export function partialNotes(p: CallQueuePartial | null): string[] {
  if (!p) return [];
  const notes: string[] = [];
  if (p.missing_contacts > 0) notes.push(`担当者情報を取得できなかった行が ${String(p.missing_contacts)} 件あります`);
  if (p.missing_companies > 0) notes.push(`会社情報を取得できなかった行が ${String(p.missing_companies)} 件あります`);
  if (p.failed.length > 0) notes.push(`取得に失敗した部分: ${[...new Set(p.failed.map(f => PARTIAL_LABELS[f] ?? 'その他の情報'))].join('、')}(関連情報を表示できない行があります)`);
  if (p.excluded.no_phone > 0) notes.push(`電話番号がどこにも無いため ${String(p.excluded.no_phone)} 件を除きました`);
  if (p.excluded.stop_reason > 0) notes.push(`架電禁止・ブロック理由があるため ${String(p.excluded.stop_reason)} 件を除きました`);
  if (p.excluded.out_of_scope > 0) notes.push(`対象外(別パイプライン・対象外ステージ・アーカイブ)の ${String(p.excluded.out_of_scope)} 件を除きました`);
  if (p.unknown_stages > 0) notes.push(`架電キューの設定に無いステージが HubSpot に ${String(p.unknown_stages)} 個あり、架電対象外として扱っています(管理者に連絡してください)`);
  return notes;
}

/** Zoom の枠を開いているか (このブラウザに残す。自動で開いたときは残さない) */
export const ZOOM_DRAWER_KEY = 'hrhr.crm.zoomDrawerOpen';
function loadDrawerOpen(): boolean {
  try { return window.localStorage.getItem(ZOOM_DRAWER_KEY) === '1'; } catch { return false; }
}
function saveDrawerOpen(open: boolean) {
  try { window.localStorage.setItem(ZOOM_DRAWER_KEY, open ? '1' : '0'); } catch { /* 残せない環境では毎回閉じた状態から */ }
}

export type ZoomReadiness = 'loading' | 'ready' | 'unavailable' | 'no_response' | 'ringing' | 'connected';
export const ZOOM_READINESS_LABELS: Record<ZoomReadiness, string> = {
  loading: '準備中', ready: '利用可', unavailable: '使えません', no_response: '応答なし', ringing: '呼び出し中', connected: '通話中',
};
/** 上のバーの「Zoom」ボタンに出す状態 */
export function zoomReadiness(embed: EmbedPhase, call: CallState, stalled: boolean): ZoomReadiness {
  if (call.phase === 'ringing') return 'ringing';
  if (call.phase === 'connected') return 'connected';
  if (embed === 'timeout' || embed === 'disabled') return 'unavailable';
  if (embed === 'loading') return 'loading';
  return stalled ? 'no_response' : 'ready';
}

const SCOPE_WORDS: Record<string, string> = { all: '全員', me: '自分', unassigned: '担当者なし' };

/**
 * 応答の scope.owner (all / me / unassigned / owner ID) を、画面の注記に出す名前にする。
 * 名前が分からない所有者 (一覧の読み込み中・失敗) は ID を画面に出さない (ID は ownerScopeTitle の tooltip に出す)
 */
export function ownerScopeLabel(scopeOwner: string, names: ReadonlyMap<string, string>): string {
  return SCOPE_WORDS[scopeOwner] ?? names.get(scopeOwner) ?? '選んだ所有者(名前を取得できません)';
}

/** 名前が分からない所有者のときだけ、tooltip に HubSpot の所有者 ID を出す */
export function ownerScopeTitle(scopeOwner: string, names: ReadonlyMap<string, string>): string | undefined {
  return SCOPE_WORDS[scopeOwner] !== undefined || names.has(scopeOwner) ? undefined : `HubSpot の所有者 ID: ${scopeOwner}`;
}

export interface ConditionChip { key: string; label: string; clear: Partial<QueueFilters>; title?: string | undefined }

const range = (from: string, to: string) => `${from ? from.replaceAll('-', '/') : ''}〜${to ? to.replaceAll('-', '/') : ''}`;

/** ステージの絞り込みのチップの文言 (3 つまでは名前、それより多いか名前が分からなければ件数) */
export function stageChipLabel(stages: readonly string[], pipeline: QueuePipelineDef | undefined): string {
  const defs = stages.map(id => pipeline?.stages.find(x => x.id === id));
  if (stages.length > 3 || defs.some(d => !d?.label)) return `ステージ: ${String(stages.length)} 件を選択`;
  return `ステージ: ${defs.map(d => (d ? stageName(d) : '')).join('、')}`;
}

/**
 * 既定と違う絞り込み条件を、外せるチップとして並べる (詳細条件を閉じていても見えるように)。
 * 並び替えとパイプラインは選択欄に出ているので含めない
 */
export function conditionChips(f: QueueFilters, ownerNames: ReadonlyMap<string, string>, pipeline?: QueuePipelineDef): ConditionChip[] {
  const chips: ConditionChip[] = [];
  if (f.q.trim()) chips.push({ key: 'q', label: `キーワード: ${f.q.trim()}`, clear: { q: '' } });
  if (f.owner) chips.push({ key: 'owner', label: `所有者: ${ownerScopeLabel(f.owner, ownerNames)}`, clear: { owner: '' }, title: ownerScopeTitle(f.owner, ownerNames) });
  if (f.due === 'today') chips.push({ key: 'due', label: '次回日が来たものだけ', clear: { due: 'all' } });
  if (f.stages.length > 0) {
    chips.push({ key: 'stages', label: stageChipLabel(f.stages, pipeline ?? findPipeline(f.pipeline)), clear: { stages: [] } });
  }
  if (f.nextFrom || f.nextTo) chips.push({ key: 'next', label: `次回架電日: ${range(f.nextFrom, f.nextTo)}`, clear: { nextFrom: '', nextTo: '' } });
  if (f.lastFrom || f.lastTo) chips.push({ key: 'last', label: `最終架電日: ${range(f.lastFrom, f.lastTo)}`, clear: { lastFrom: '', lastTo: '' } });
  return chips;
}

/** 架電結果の印 (この画面のタブだけに残る) の説明 */
export const RECORDED_TITLE = 'この画面(タブ)だけに残ります。タブを閉じると消え、HubSpot には保存されません';
export const UNSAVED_RECORDED_TITLE = '入力をこの画面に残せていません。閉じたり再読み込みしたりすると消えます。HubSpot にも保存されていません';


/** 行は props が同じなら描き直さない (架電結果を 1 文字打つたび・矢印キーで 1 行動くたびに全行を描き直さない) */
const QueueRow = memo(function QueueRow({ item, ownerName, selected, focusable, recorded, unsaved, stopLabel, onSelect }: {
  item: CallQueueItem; ownerName?: string | undefined; selected: boolean; focusable: boolean; recorded: boolean; unsaved: boolean;
  stopLabel: StopLabel; onSelect: (id: string) => void;
}) {
  const phone = formatPhoneForDisplay(item.phone);
  const next = md(item.next_call_date);
  const last = md(item.last_call_date);
  const dates = [ymd(item.next_call_date) && `次回架電 ${ymd(item.next_call_date) ?? ''}`, ymd(item.last_call_date) && `最終架電 ${ymd(item.last_call_date) ?? ''}`]
    .filter(Boolean).join(' / ');
  const flag = item.stop.unreachable_check ? stopLabel('bpo_10', item.stop.unreachable_check) : null;
  const source = item.phone_source ? `${PHONE_SOURCE_LABELS[item.phone_source] ?? item.phone_source}の番号` : undefined;
  return <li className={`cq-row${selected ? ' is-selected' : ''}`}>
    <button type="button" className="cq-row-button" aria-pressed={selected} tabIndex={focusable ? 0 : -1}
      title={item.deal_name ?? undefined} onClick={() => { onSelect(item.deal_id); }}>
      <span className="cq-row-l1">
        <strong className="cq-row-company">{item.company?.name ?? <span className="crm-muted">会社情報を取得できませんでした</span>}</strong>
        {recorded && (unsaved
          ? <span className="cq-recorded is-unsaved" title={UNSAVED_RECORDED_TITLE}>記録済み(画面を閉じると消えます)</span>
          : <span className="cq-recorded" title={RECORDED_TITLE}>記録済み(HubSpot 未送信)</span>)}
        {flag && <span className="cq-flag" title={`不通時チェック: ${flag}`} aria-label={`不通時チェック: ${flag}`}>不通チェック</span>}
        <span className="cq-stage">{item.stage_label ?? '(ステージ不明)'}</span>
      </span>
      <span className="cq-row-l2">
        {item.contact ? <span className="cq-row-contact">{item.contact.name ?? '(氏名なし)'}
          {item.contact.extra_count > 0 && <small> ほか{item.contact.extra_count}人</small>}</span>
          : <span className="crm-muted">担当者情報を取得できませんでした</span>}
        <span className="cq-sep" aria-hidden="true">·</span>
        {phone ? <span className="cq-phone" title={item.phone ?? undefined} data-source={source}>{phone}</span>
          : <span className="crm-muted">番号を確認できません</span>}
      </span>
      <span className="cq-row-l3" title={dates || undefined}>
        <span>次回 {next ? <><span>{next}</span>{item.next_call_time && <> <span>{item.next_call_time}</span></>}</> : 'なし'}</span>
        <span>最終 {last ? <span>{last}</span> : '未架電'}</span>
        <span>担当 {item.owner_id ? <span title={ownerName ? undefined : `HubSpot の所有者 ID: ${item.owner_id}`}>{ownerName ?? '担当あり'}</span> : 'なし'}</span>
      </span>
    </button>
  </li>;
});

export function CallQueueScreen({ fetcher, ownersFetcher, detailFetcher, metadataFetcher, userFetcher, pipelinesFetcher, catalogFetcher, writeApi, writePollMs, zoomOptions, initialSearch, now }: {
  fetcher?: QueueFetch; ownersFetcher?: OwnersFetch; detailFetcher?: DetailFetch; metadataFetcher?: MetadataFetch; userFetcher?: UserFetch;
  pipelinesFetcher?: PipelinesFetch; catalogFetcher?: CatalogFetch;
  /** 項目の書き換え (省略すると、実データは /api/crm、架空サンプルはメモリの中だけ) */
  writeApi?: WriteApi; writePollMs?: number;
  zoomOptions?: ZoomOptions | undefined; initialSearch?: string; now?: () => number;
}) {
  const search = initialSearch ?? window.location.search;
  const [mode, setMode] = useState<QueueMode>(() => parseMode(search));
  const [filters, setFilters] = useState<QueueFilters>(() => parseFilters(search));
  const [qDraft, setQDraft] = useState(filters.q);
  const [panelOpen, setPanelOpen] = useState(false);
  // 日付の検証に使う JST の今日。JST 0 時・画面に戻ったときに取り直す (開いたまま日付をまたいでも昨日を通さない)
  const [nowFn] = useState(() => now ?? Date.now);
  const [today, setToday] = useState(() => todayJst(nowFn()));
  // 「次回日が来たものだけ」は今日で決まるので、日付が変わったら一覧も取り直す
  const { state, loadMore, reload } = useCallQueue(filters, mode, fetcher, filters.due === 'today' ? today : '');
  // 選んだ案件。モードを切り替えたら選び直す (実データの ID と架空の ID を取り違えない)
  const [selection, setSelection] = useState<{ id: string; mode: QueueMode } | null>(null);
  const selectedId = selection !== null && selection.mode === mode ? selection.id : null;
  // 詳細を読む案件。クリックはすぐ、矢印キーは少し待ってから (selection と違う間は古い詳細を出さない)
  const [detailSel, setDetailSel] = useState<{ id: string; mode: QueueMode } | null>(null);
  const detailId = detailSel !== null && detailSel.mode === mode ? detailSel.id : null;
  const keyTimer = useRef<number | null>(null);
  // パネルの配置 (このブラウザに残す。変えても HubSpot は呼ばない)
  // 以前の版の配置だけが残っていたら新しい既定の配置にして、1 回だけ案内を出す (新しい配置を残せたら以前の配置は消す)
  const [initialLayout] = useState(() => loadLayoutWithNotice(localStorageOrNull()));
  const [layout, dispatchLayout] = useReducer(dockReducer, initialLayout.layout);
  const [layoutNote, setLayoutNote] = useState(initialLayout.migrated);
  useEffect(() => { if (saveLayout(localStorageOrNull(), layout)) clearLegacyLayout(localStorageOrNull()); }, [layout]);
  // 「案件の概要」の 1 行表示: 列が低い (画面の高さが低い) ときは自動で 1 行。利用者が切り替えたらそれに従う
  const shortScreen = useMediaQuery(COMPACT_OVERVIEW_QUERY);
  const [densityPref, setDensityPref] = useState<'auto' | 'full' | 'compact'>('auto');
  const compactOverview = densityPref === 'auto' ? shortScreen : densityPref === 'compact';
  const density = useMemo(() => ({ compact: compactOverview, onToggle: () => { setDensityPref(compactOverview ? 'full' : 'compact'); } }), [compactOverview]);
  // 「プロパティ」パネルで表示する項目 (このブラウザに残す)。項目の一覧に無いもの (HubSpot で消された等) は送らない
  const [storedProps, setStoredProps] = useState<SelectedProps>(() => loadSelected(localStorageOrNull()));
  const catalog = usePropertyCatalog(mode, selection !== null, catalogFetcher);
  const selectedProps = useMemo(() => (catalog.state.phase === 'ready' ? sanitizeSelected(storedProps, catalog.state.index) : storedProps), [catalog.state, storedProps]);
  const applyProps = useCallback((next: SelectedProps) => { setStoredProps(next); saveSelected(localStorageOrNull(), next); }, []);
  // 通話が終わったが、まだ読み直していない案件 (次に開くときサーバのキャッシュを使わずに読む)
  const [staleDeals, setStaleDeals] = useState<ReadonlySet<string>>(() => new Set());
  const freshLoaded = useCallback((id: string) => {
    setStaleDeals(prev => {
      if (!prev.has(id)) return prev;
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  }, []);
  const detail = useDealDetail(detailId, mode, detailFetcher, selectedProps, staleDeals, freshLoaded);
  // 項目の一覧を読み終える前に送った項目に、一覧に無いもの (HubSpot で隠された・消された項目) があると 400 invalid_properties になる。
  // 一覧を読み終えると一覧に無い項目を外して読み直すので、その間は失敗を出さずに「読み込み中」とする
  const detailState = useMemo(() => (detail.state.phase === 'error' && detail.state.errorKind === 'invalid_properties' && catalog.state.phase === 'loading'
    ? { ...detail.state, phase: 'loading' as const, message: '' } : detail.state), [detail.state, catalog.state.phase]);
  // Zoom Phone は常駐 (案件を切り替えても作り直さない)。架空サンプルでは出さず、発信もしない
  const { zoom, iframeRef } = useZoomPhone(mode === 'live', zoomOptions);
  // Zoom の枠は右から開く引き出し。閉じている間も iframe は画面の外に置いたまま (発信の依頼は届く)
  const [drawerOpenRaw, setDrawerOpen] = useState(loadDrawerOpen);
  const drawerOpen = drawerOpenRaw && mode === 'live';
  const zoomToggleRef = useRef<HTMLButtonElement | null>(null);
  const drawerRef = useRef<HTMLDivElement | null>(null);
  // 開け閉めの後にフォーカスを移す先 (利用者が開けたときは枠の中へ、閉じたときは「Zoom」ボタンへ)
  const focusAfterToggle = useRef<'drawer' | 'toggle' | null>(null);
  const openZoom = useCallback(() => { focusAfterToggle.current = 'drawer'; setDrawerOpen(true); saveDrawerOpen(true); }, []);
  const closeZoom = useCallback(() => { focusAfterToggle.current = 'toggle'; setDrawerOpen(false); saveDrawerOpen(false); }, []);
  // 自動で開く (Zoom が応答しない・一度も応答していない)。フォーカスは奪わず、選んだ開け閉めとしても残さない
  const autoOpenZoom = useCallback(() => { setDrawerOpen(true); }, []);
  useEffect(() => {
    const target = focusAfterToggle.current;
    focusAfterToggle.current = null;
    if (target === 'drawer' && drawerOpen) drawerRef.current?.querySelector<HTMLElement>('.zp-close')?.focus();
    if (target === 'toggle' && !drawerOpen) zoomToggleRef.current?.focus();
  }, [drawerOpen]);
  // 発信したのに Zoom が応答しないときは枠を開く (サインインやアプリの起動が要る)
  const [seenStalled, setSeenStalled] = useState(false);
  if (zoom.stalled !== seenStalled) {
    setSeenStalled(zoom.stalled);
    if (zoom.stalled) setDrawerOpen(true);
  }
  const readiness = zoomReadiness(zoom.embed, zoom.call, zoom.stalled);
  const listRef = useRef<HTMLUListElement | null>(null);
  const scrollSelectedRow = useRef(false);
  // 記録して次へで移った案件 (その入力欄が開いたら結果のボタンへフォーカスする)
  const [focusFormFor, setFocusFormFor] = useState<string | null>(null);
  // 画面全体の読み上げ欄 (入力欄は案件ごとに作り直すので外に置く)。同じ文言でも読み上げ直すよう n を変える
  const [announcement, setAnnouncement] = useState<{ text: string; n: number }>({ text: '', n: 0 });
  const announce = useCallback((text: string) => { setAnnouncement(a => ({ text, n: a.n + 1 })); }, []);

  // 架電結果の下書き (案件ごと、このタブの sessionStorage に残す。HubSpot には送らない)
  // 保存した人 (ログイン中のメールアドレス) も一緒に残し、その人にだけ戻す (共用の PC で、同じタブで別の人がログインし直しても前の人のメモを見せない)。
  // 誰がログインしているか分かった時点で 1 回読み、書き込めるかを試す。以後は変えるたびに書いた結果で更新する。
  // 残せなかったら (誰か分からないときも) 画面に赤で出す (headless-crm-design §12。保存できたように見せない)
  const currentUser = useCurrentUser(userFetcher);
  const storeOwner = currentUser.phase === 'ready' ? currentUser.email : currentUser.phase === 'error' ? null : undefined;
  const [{ store, persistFailed, loadedFor }, setPersisted] = useState<{ store: DraftStore; persistFailed: boolean; loadedFor: string | null | undefined }>(
    () => ({ store: emptyStore(), persistFailed: false, loadedFor: undefined }));
  if (storeOwner !== loadedFor) {
    if (storeOwner === null) setPersisted(p => ({ ...p, persistFailed: true, loadedFor: null }));
    else if (storeOwner !== undefined) {
      const initial = loadStore(sessionStorageOrNull(), storeOwner);
      setPersisted({ store: initial, persistFailed: !saveStore(sessionStorageOrNull(), initial, storeOwner), loadedFor: storeOwner });
    }
  }
  const storeReady = loadedFor !== undefined;
  const commitStore = (next: DraftStore) => {
    setPersisted({ store: next, persistFailed: loadedFor === null || loadedFor === undefined || !saveStore(sessionStorageOrNull(), next, loadedFor), loadedFor });
  };
  const [formCollapsed, setFormCollapsed] = useState(false);
  useEffect(() => {
    const refresh = () => { setToday(todayJst(nowFn())); };
    const t = window.setTimeout(refresh, msUntilNextJstMidnight(nowFn()) + 1000);
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', refresh);
    return () => { window.clearTimeout(t); window.removeEventListener('focus', refresh); document.removeEventListener('visibilitychange', refresh); };
  }, [today, nowFn]);
  const [formNotice, setFormNotice] = useState<{ dealId: string; text: string } | null>(null);
  const defs = useResultDefinitions(mode, selectedId !== null, metadataFetcher);
  // どの案件の画面から発信したか (通話の終了を、その発信の通話についてだけ、その案件の入力欄に出す)
  const [dialedFor, setDialedFor] = useState<DialedFor | null>(null);
  if (dialedFor !== null && bindsToDial(dialedFor, zoom.call)) setDialedFor({ ...dialedFor, callId: zoom.call.callId });
  // 画面から発信した通話が終わったら、その案件の詳細はサーバのキャッシュ (60 秒) を使わずに読み直す。
  // 表示中ならすぐ (前の内容は出したまま)、別の案件を見ていれば次に開いたときに
  const endedDial = zoom.call.phase === 'ended' && dialedFor !== null && dialedFor.callId !== null
    && dialedFor.callId === zoom.call.callId ? dialedFor : null;
  const endedDialKey = endedDial === null ? null : `${endedDial.mode}|${endedDial.dealId}|${String(endedDial.callId)}`;
  const [seenEndedDial, setSeenEndedDial] = useState<string | null>(null);
  if (endedDialKey !== seenEndedDial) {
    setSeenEndedDial(endedDialKey);
    if (endedDial !== null) {
      if (endedDial.mode === mode && endedDial.dealId === detailId) detail.refresh();
      else setStaleDeals(prev => new Set(prev).add(endedDial.dealId));
    }
  }
  // 結果のボタンへフォーカスを移し終えた通話 (同じ通話で何度もフォーカスを奪わない)
  const [handledCall, setHandledCall] = useState<string | null>(null);
  const zoomForDetail = useMemo<ZoomPhone>(() => ({
    ...zoom,
    dial: (raw: string | null | undefined): DialResult => {
      const r = zoom.dial(raw);
      // 読み込んでから Zoom が一度も何も言ってこない (サインインしていない可能性が高い) まま発信したら、枠を開いて見せる
      if (r !== 'not_dialable' && !zoom.heard) autoOpenZoom();
      if (r === 'sent' && detailId !== null) {
        setDialedFor({ dealId: detailId, mode, number: toE164Jp(raw), staleCallId: zoom.call.callId, callId: null });
      }
      return r;
    },
  }), [zoom, detailId, mode, autoOpenZoom]);

  useEffect(() => () => { if (keyTimer.current !== null) window.clearTimeout(keyTimer.current); }, []);

  const select = useCallback((id: string, via: 'click' | 'key') => {
    const sel = { id, mode };
    setSelection(sel);
    setFocusFormFor(null);
    if (keyTimer.current !== null) { window.clearTimeout(keyTimer.current); keyTimer.current = null; }
    if (via === 'click') { setDetailSel(sel); return; }
    keyTimer.current = window.setTimeout(() => { keyTimer.current = null; setDetailSel(sel); }, KEY_SELECT_DELAY_MS);
  }, [mode]);
  // 行に渡すクリック時の選択 (同じ関数を渡し続け、行の描き直しを避ける)
  const selectByClick = useCallback((id: string) => { select(id, 'click'); }, [select]);

  function update(patch: Partial<QueueFilters>) {
    setFilters(prev => {
      const next = { ...prev, ...patch };
      return filtersKey(next) === filtersKey(prev) ? prev : next;
    });
  }

  // キーワードは入力が止まってから反映する (1 文字ごとに HubSpot を呼ばない)
  useEffect(() => {
    const t = window.setTimeout(() => { update({ q: qDraft }); }, 400);
    return () => { window.clearTimeout(t); };
  }, [qDraft]);

  // 条件を URL に残す (再読み込みで復元)。履歴は増やさない
  useEffect(() => {
    if (initialSearch !== undefined) return;
    try { window.history.replaceState(null, '', screenSearch(filters, mode) || window.location.pathname); } catch { /* URL を書けない環境では何もしない */ }
  }, [filters, mode, initialSearch]);

  // 「条件をクリア」はパイプラインを残す (選んだパイプラインの中で条件だけ戻す)
  const hasConditions = useMemo(() => filtersKey(filters) !== filtersKey({ ...DEFAULT_FILTERS, pipeline: filters.pipeline }), [filters]);
  const pipelines = useQueuePipelines(mode, pipelinesFetcher);
  const pipeline = pipelines.pipelines.find(p => p.id === filters.pipeline) ?? findPipeline(filters.pipeline);
  // 所有者の既定 (条件の owner が '' のとき、サーバが実際に使った所有者 = 管理者は all、それ以外は me)。
  // 条件を変えて読み直している間は応答が空になるので、同じモードで分かった値を覚えておく (選択欄がちらつかないように)
  const [seenOwner, setSeenOwner] = useState<{ mode: QueueMode; owner: string } | null>(null);
  if (state.last !== null && filters.owner === '' && (seenOwner?.mode !== mode || seenOwner.owner !== state.last.scope.owner)) {
    setSeenOwner({ mode, owner: state.last.scope.owner });
  }
  const effectiveOwner = seenOwner?.mode === mode ? seenOwner.owner : null;
  // 所有者の一覧は CRM の利用者全員が使える。実データでは HubSpot、架空サンプルでは架空の一覧
  const owners = useOwners(true, mode === 'fixture' ? fixtureOwnersFetch : (ownersFetcher ?? liveOwnersFetch));
  const ownerNames = useMemo(() => ownerNameMap(owners.state.phase === 'ready' ? owners.state.owners : []), [owners.state]);
  const needsOwnerPick = state.phase === 'error' && state.errorKind === 'owner_not_resolved';
  const notes = partialNotes(state.partial);
  const total = state.total;
  // 一覧を下端近くまでスクロールしたら続きを読む (読み込み中・失敗中・続きなしは読まない)。ボタンでも読める
  const listScrollRef = useRef<HTMLDivElement | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);
  useAutoLoadMore(listScrollRef, sentinelRef,
    state.phase === 'ready' && state.nextCursor !== null && !state.loadingMore && state.moreError === null,
    state.nextCursor, loadMore);
  // 条件を変えて取り直したら一覧の枠を先頭に戻す (前の一覧の下端の位置のままだと、スクロールしていないのに続きを読んでしまう)
  useEffect(() => { if (listScrollRef.current) listScrollRef.current.scrollTop = 0; }, [state.reqId]);
  const chips = conditionChips(filters, ownerNames, pipeline);
  const detailCount = (filters.stages.length > 0 ? 1 : 0) + (filters.nextFrom || filters.nextTo ? 1 : 0) + (filters.lastFrom || filters.lastTo ? 1 : 0);

  /** パイプラインの切り替え: ステージの選択は既定 (すべて) に戻し、先頭から読み直す */
  function changePipeline(id: string) {
    setFilters(prev => (prev.pipeline === id ? prev : withPipeline(prev, id)));
  }
  /** モードの切り替え: そのモードで選べないパイプラインは既定に戻す (架空のパイプラインを実データに送らない) */
  function switchMode(next: QueueMode) {
    setMode(next);
    if (!pipelinesFor(next).some(p => p.id === filters.pipeline)) changePipeline(DEFAULT_PIPELINE_ID);
  }
  function removeChip(c: ConditionChip) {
    if (c.clear.q !== undefined) setQDraft('');
    update(c.clear);
  }
  function clearAll() { setQDraft(''); setFilters(prev => ({ ...DEFAULT_FILTERS, pipeline: prev.pipeline })); }

  // 一覧にフォーカスがあるとき、上下の矢印キーで選択を移す
  function onListKey(e: React.KeyboardEvent<HTMLUListElement>) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (state.phase !== 'ready' || state.items.length === 0) return;
    e.preventDefault();
    const items = state.items;
    // 行のボタンは 1 回だけ集める (行ごとに一覧全体を探さない)
    const buttons = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>('.cq-row-button') ?? []);
    const focused = buttons.findIndex(b => b === document.activeElement);
    const cur = focused !== -1 ? focused : items.findIndex(i => i.deal_id === selectedId);
    const step = e.key === 'ArrowDown' ? 1 : -1;
    const nextIdx = cur === -1 ? (step === 1 ? 0 : items.length - 1) : Math.min(items.length - 1, Math.max(0, cur + step));
    const target = items[nextIdx];
    if (!target) return;
    if (target.deal_id !== selectedId) select(target.deal_id, 'key');
    const btn = buttons[nextIdx];
    btn?.focus();
    if (typeof btn?.scrollIntoView === 'function') btn.scrollIntoView({ block: 'nearest' });
  }

  const isRecorded = useCallback((id: string) => store.recorded[draftKey(mode, id)] === true, [store, mode]);
  const selKey = selectedId !== null ? draftKey(mode, selectedId) : null;
  const draft: ResultDraft = (selKey !== null ? store.drafts[selKey] : undefined) ?? emptyResultDraft();
  const endedCall = zoom.call.phase === 'ended' && dialedFor !== null && dialedFor.mode === mode && dialedFor.dealId === selectedId
    && dialedFor.callId !== null && dialedFor.callId === zoom.call.callId ? zoom.call : null;
  // 一覧が表示されていない (読み込み中・失敗) か、条件を変えて選んだ案件がいまの一覧から外れた。
  // どちらも記録はさせない (一覧で見えない案件を記録して次へ進まない。次の案件を一覧から選べない)
  const listReady = state.phase === 'ready';
  const offList = listReady && selectedId !== null && !state.items.some(i => i.deal_id === selectedId);
  const recordBlockedNotice = !listReady
    ? (state.phase === 'loading' ? '一覧を読み込み中です。一覧が表示されてから記録してください。' : '一覧を表示できていないため記録できません。一覧を表示してから記録してください。')
    : offList ? 'この案件はいまの一覧にありません(条件で外れました)。記録するには一覧に戻してください。' : null;
  // 発信した案件に結果を記録した後の通話 (別の案件に移っても「結び付いていない」とは言わない)
  const callRecorded = zoom.call.phase === 'ended' && dialedFor !== null && dialedFor.mode === mode && dialedFor.callId !== null
    && dialedFor.callId === zoom.call.callId && isRecorded(dialedFor.dealId);
  // Zoom の通話中にデータを切り替えると枠が外れて通話が切れるので、切り替えさせない
  const inCall = zoom.call.phase === 'ringing' || zoom.call.phase === 'connected';
  // 「架ける番号」の下に出す通話の様子。呼び出し中・通話中はどの案件でも出す (電話は 1 つ)。
  // 発信の待ち・失敗と終わった通話は、その案件から発信したものだけ
  const dialedHere = dialedFor !== null && dialedFor.mode === mode && dialedFor.dealId === selectedId;
  const waitingDial = dialedHere && dialedFor.callId === null;
  const callBar = useMemo<CallBarInfo | null>(() => {
    const c = zoom.call;
    if (c.phase === 'ringing') return { kind: 'ringing', number: c.number, inbound: c.direction !== null && c.direction !== 'outbound' };
    if (c.phase === 'connected') return { kind: 'connected', number: c.number, connectedAt: c.connectedAt };
    if (waitingDial && zoom.stalled) return { kind: 'failed' };
    if (waitingDial && zoom.pending !== null) return { kind: 'dialing', number: zoom.pending.number };
    if (endedCall !== null) return { kind: 'ended', talkSeconds: endedCall.talkSeconds, result: endedCall.result };
    return null;
  }, [zoom.call, zoom.stalled, zoom.pending, waitingDial, endedCall]);

  function changeDraft(d: ResultDraft) {
    if (selKey === null || selectedId === null) return;
    // 記録した後に書き換えたら、記録済みの印は外す (記録したときの内容と変わったため)。外したことは入力欄の下端で知らせる
    if (store.recorded[selKey] === true) setFormNotice({ dealId: selectedId, text: '内容を変えたので「記録済み」の印を外しました。もう一度「記録して次へ」を押してください。' });
    commitStore(editDraft(store, selKey, d));
  }
  function clearDraft() {
    if (selKey === null) return;
    commitStore(clearDraftEntry(store, selKey));
    setFormNotice(null);
  }
  // 記録して次へ: このブラウザで記録済みの印を付け (下書きは残す)、一覧で次の未記録の案件を選ぶ。HubSpot には送らない
  function recordAndNext(): boolean {
    if (selKey === null || selectedId === null || !listReady || offList) return false;
    // 日付の検証は記録する時点の今日で (画面を開いたまま日付をまたいだとき)
    const fresh = todayJst(nowFn());
    if (fresh !== today) {
      setToday(fresh);
      if (defs.state.phase !== 'ready' || Object.keys(validateResultDraft(draft, defs.state.defs, fresh)).length > 0) return false;
    }
    commitStore(markRecorded(store, selKey));
    const ids = state.items.map(i => i.deal_id);
    const next = nextUnrecorded(ids, selectedId, isRecorded);
    const name = state.items.find(i => i.deal_id === selectedId)?.company?.name ?? 'この架電先';
    const done = `${name} を記録しました(この画面だけ。HubSpot には未送信)。`;
    if (next === null) {
      // 続きのページがあるときは、一覧が終わったと読まれないよう続きの出し方も書く
      const none = state.nextCursor
        ? '表示中の一覧に未記録の架電先はありません。一覧の下の「さらに読み込む」で続きを表示できます。'
        : '表示中の一覧に未記録の架電先はありません。';
      setFormNotice({ dealId: selectedId, text: none });
      announce(`${done}${none}`);
      return true;
    }
    setFormNotice(null);
    announce(`${done}次の架電先を表示しています。`);
    select(next, 'click');
    // 次の案件の入力欄が開いたら、結果のボタンへフォーカスする (続けてキーボードで入力できるように)
    setFocusFormFor(next);
    scrollSelectedRow.current = true;
    return true;
  }
  // 記録して次へで選び直したら、一覧の選んだ行を見える位置まで送る (フォーカスは入力欄の結果のボタンへ)
  useEffect(() => {
    if (!scrollSelectedRow.current) return;
    scrollSelectedRow.current = false;
    const btn = listRef.current?.querySelector<HTMLButtonElement>('.cq-row-button[aria-pressed="true"]');
    if (typeof btn?.scrollIntoView === 'function') btn.scrollIntoView({ block: 'nearest' });
  }, [selectedId]);

  const waitingKey = selectedId !== null && selectedId !== detailId;
  // 「求人検索・リンク先」パネルの中のタブ (リンク一覧 / 求人検索 / 開いたリンク)。案件を選び直したら開いたリンクは閉じる
  const loadedData = !waitingKey && detailState.phase === 'ready' && detailState.data?.deal.id === selectedId ? detailState.data : null;
  // 項目の書き換え。保存した値は、読み直した詳細に切り替わるまで仮に重ねて表示する
  const effectiveWriteApi = writeApi ?? (mode === 'fixture' ? fakeWriteApi : liveWriteApi);
  const crmWrite = useCrmWrite({
    dealId: detailId, api: effectiveWriteApi, pollIntervalMs: writePollMs,
    onSaved: id => { if (id === detailId) detail.refresh(); },
  });
  const loadWriteValues = useCallback(async (id: string, defs: readonly FieldDef[]) => {
    const names = (o: FieldDef['object']) => defs.filter(d => d.object === o).map(d => d.name);
    const props: SelectedProps = { deals: names('deal'), contacts: names('contact'), companies: names('company') };
    const fetchDetail = detailFetcher ?? (mode === 'fixture' ? fixtureDetailFetch : liveDetailFetch);
    const r = await fetchDetail(id, new AbortController().signal, props, { fresh: true });
    if (!r.ok) throw new Error('detail');
    const out: Record<string, string | null> = {};
    for (const d of defs) out[d.name] = (d.object === 'deal' ? r.data.selected.deal : d.object === 'contact' ? r.data.selected.contact : r.data.selected.company)[d.name] ?? null;
    return out;
  }, [detailFetcher, mode]);
  const writeBindings = useWriteBindings({
    write: crmWrite, dealId: detailId, data: loadedData, index: catalog.state.phase === 'ready' ? catalog.state.index : null,
    ownerNames, loadValues: loadWriteValues,
  });
  const shownData = writeBindings.view;
  const searchUrl = useMemo(() => (shownData !== null ? dealJobSearchUrl(shownData) : null), [shownData]);
  const center = useCenterTabs(selectedId === null ? null : `${mode}:${selectedId}`, searchUrl);
  const searchLink = useMemo(() => (searchUrl !== null ? searchTab(searchUrl) : null), [searchUrl]);
  // 通話が終わったら「架電結果の入力」を前に出す (どの列に置いていても。リンクのタブは閉じない)
  const endedKey = endedCall === null ? null : (endedCall.callId ?? 'ended');
  const [seenEnded, setSeenEnded] = useState<string | null>(null);
  if (endedKey !== seenEnded) {
    setSeenEnded(endedKey);
    if (endedKey !== null) dispatchLayout({ type: 'activate', panel: 'result' });
  }
  // リンクを開いたら「求人検索・リンク先」パネルを前に出す
  const centerOpen = center.open;
  const openLinkInPanel = useCallback((url: string, label?: string) => {
    centerOpen(url, label);
    dispatchLayout({ type: 'activate', panel: 'links' });
  }, [centerOpen]);
  const layoutDispatch = useCallback((a: DockAction) => { dispatchLayout(a); }, []);
  // 「求人検索・リンク先」を置き場全体に広げる (Esc か「戻す」で戻る)。列を重ねるだけなので枠の中のページは読み直さない。
  // 同じ列で別のタブを前に出した・架電先の選択を外したときは戻す
  const [maxLinks, setMaxLinks] = useState(false);
  const linksFront = layout.columns[columnOf(layout, 'links')].active === 'links';
  if (maxLinks && (!linksFront || selectedId === null)) setMaxLinks(false);
  useEffect(() => {
    if (!maxLinks) return;
    // メニュー・Zoom の枠が Esc を使ったとき (preventDefault 済み) は戻さない
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape' && !e.defaultPrevented) { e.preventDefault(); setMaxLinks(false); } };
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('keydown', onKey); };
  }, [maxLinks]);
  // 不通時チェック・ブロック理由は、入力欄と同じ HubSpot の表示ラベルで出す (定義を読めていなければ値のまま)
  const stopLabel = useMemo<StopLabel>(() => {
    if (defs.state.phase !== 'ready') return rawStopLabel;
    const d = defs.state.defs;
    return (p, v) => optionLabel(d[p], v);
  }, [defs.state]);
  const anyFocusable = state.items.some(i => i.deal_id === selectedId);

  const detailPhase = waitingKey ? 'waiting' : detailState.phase;
  const placeholder = panelPlaceholder(selectedId !== null, detailPhase);
  const panels: Record<PanelId, React.ReactNode> = {
    queue: <>
        <section className="cq-col cq-list-col" aria-label="架電先の一覧" aria-busy={state.phase === 'loading'}>
          <div className="cq-list-head">
            {state.phase === 'ready' && <p className="cq-count" role="status" data-testid="queue-count"
              title={total !== null ? QUEUE_TOTAL_NOTE : undefined}>{queueCountText(total, state.items.length)}
              {total !== null && <span>({QUEUE_TOTAL_NOTE_SHORT})</span>}</p>}
            {mode === 'live' && state.last !== null && <p className="cq-scope-note" data-testid="scope-note"
              title={[ownerScopeTitle(state.last.scope.owner, ownerNames), 'HubSpot の全件から、上の所有者の選択で切り替えられます'].filter(Boolean).join('。')}>所有者: {ownerScopeLabel(state.last.scope.owner, ownerNames)} を表示中</p>}
          </div>
          <div className="cq-list-scroll" ref={listScrollRef}>
            {state.phase === 'invalid' && <div className="cq-notice cq-error" role="alert"><strong>条件を確認してください</strong>
              <ul>{state.invalid.map(m => <li key={m}>{m}</li>)}</ul></div>}
            {state.phase === 'loading' && <p role="status" className="cq-loading">読み込み中…</p>}
            {state.phase === 'unauthorized' && <div className="cq-notice cq-error" role="alert"><strong>表示できません</strong><p>{state.message}</p>
            </div>}
            {state.phase === 'error' && needsOwnerPick && <div className="cq-notice cq-warn" role="status" data-testid="owner-pick-prompt">
              <strong>所有者を選んでください</strong><p>{state.message}</p></div>}
            {state.phase === 'error' && !needsOwnerPick && <div className="cq-notice cq-error" role="alert"><strong>取得できませんでした</strong><p>{state.message}</p>
              <button type="button" onClick={reload}>再試行</button></div>}

            {state.phase === 'ready' && <>
              {state.last?.truncated && <div className="cq-notice cq-warn" role="status">HubSpot の検索は 1 万件までしか取得できないため、これより先は表示できません。条件を絞ってください。</div>}
              {notes.length > 0 && <div className="cq-notice cq-warn" role="status"><strong>一部の情報が欠けています</strong>
                <ul>{notes.map(n => <li key={n}>{n}</li>)}</ul></div>}
              {state.items.length === 0 && <div className="cq-notice cq-empty">
                {state.nextCursor ? <p>このページには表示できる行がありません。続きを読み込んでください。</p>
                  : hasConditions ? <><strong>条件に一致する架電先がありません</strong><p>条件を変えるか、クリアしてください。</p>
                    <button type="button" onClick={clearAll}>条件をクリア</button></>
                    : <><strong>いま架電キューに出ている架電先はありません</strong></>}</div>}
              {state.items.length > 0 && <ul className="cq-list" aria-label="架電キュー" ref={listRef} onKeyDown={onListKey}>
                {state.items.map((item, i) => <QueueRow key={item.deal_id} item={item}
                  selected={item.deal_id === selectedId} focusable={anyFocusable ? item.deal_id === selectedId : i === 0}
                  recorded={isRecorded(item.deal_id)} unsaved={persistFailed} stopLabel={stopLabel}
                  onSelect={selectByClick} ownerName={item.owner_id ? ownerNames.get(item.owner_id) : undefined} />)}</ul>}
              {state.moreError && <div className="cq-notice cq-error" role="alert"><strong>続きを読み込めませんでした</strong><p>{state.moreError.message}</p>
                {state.moreError.kind === 'cursor_mismatch' && <button type="button" onClick={reload}>最初から読み直す</button>}</div>}
              {/* 続きの自動読み込みの目印 (ここが見えるところまでスクロールしたら読む) */}
              {state.nextCursor && <div ref={sentinelRef} className="cq-load-sentinel" aria-hidden="true" data-testid="queue-load-sentinel" />}
              {state.nextCursor && <button type="button" className="cq-btn cq-load-more" disabled={state.loadingMore} onClick={loadMore}>
                {state.loadingMore ? '読み込み中…' : 'さらに読み込む'}</button>}
              {!state.nextCursor && state.items.length > 0 && <p className="cq-end">これで最後です。</p>}
            </>}
          </div>
        </section>
    </>,
    properties: <PropertyPanel catalog={catalog.state} onReloadCatalog={catalog.reload} selection={selectedProps} onApply={applyProps}
      data={shownData} placeholder={placeholder} ownerNames={ownerNames} hasSelection={selectedId !== null} write={writeBindings.panel} />,
    overview: <section className="cq-col cq-detail" aria-label="選んだ架電先の詳細">
      {waitingKey ? <div className="cq-detail-scroll"><p role="status" className="cq-loading">詳細を読み込み中…</p></div>
        : <DealOverview state={detailState} reload={detail.reload} refresh={detail.refresh} zoom={zoomForDetail} stopLabel={stopLabel}
          callBar={callBar} onOpenZoom={mode === 'live' ? openZoom : undefined} density={density} stageMove={writeBindings.stage} />}
    </section>,
    activity: <ActivityLog data={shownData} placeholder={placeholder} ownerNames={ownerNames} />,
    // 架電結果の入力欄。案件を選んでいるときだけ出す。下書きだけで HubSpot には送らない
    result: selectedId === null ? <div className="dock-scroll"><p className="dock-placeholder">{placeholder}</p></div>
      : <div className="cq-result-slot" data-testid="result-slot" data-deal-id={selectedId}>
        {!storeReady ? <p role="status" className="cq-loading">架電結果の入力欄を準備しています…</p> : <CallResultForm key={`${mode}:${selectedId}`} dealId={selectedId} draft={draft} onChange={changeDraft}
          defsState={defs.state} onReloadDefs={defs.reload} recorded={isRecorded(selectedId)} onRecord={recordAndNext} onClear={clearDraft}
          collapsed={formCollapsed} onCollapsedChange={setFormCollapsed} endedCall={endedCall} today={today}
          focusCallId={endedCall?.callId != null && endedCall.callId !== handledCall ? endedCall.callId : null} onCallHandled={setHandledCall}
          recordBlocked={recordBlockedNotice !== null} persistFailed={persistFailed}
          autoFocusOutcome={focusFormFor === selectedId} onAnnounce={announce}
          notice={recordBlockedNotice ?? (formNotice?.dealId === selectedId ? formNotice.text : undefined)} />}
      </div>,
    links: selectedId === null ? <div className="dock-scroll"><p className="dock-placeholder">{placeholder}</p></div> : <div className="cq-linkpanel">
      <div className="cq-linkpanel-head">
        <CenterTabBar tabs={center} />
        <button type="button" className="cq-maximize" data-testid="links-maximize"
          title={maxLinks ? '元の大きさに戻します(Esc でも戻せます)' : 'このパネルを画面いっぱいに広げます(Esc か「戻す」で戻ります)'}
          onClick={() => { if (!maxLinks) dispatchLayout({ type: 'activate', panel: 'links' }); setMaxLinks(m => !m); }}>{maxLinks ? '戻す' : '広げる'}</button>
      </div>
      <div className="cq-cpanels">
        <CenterPanel id={DEAL_TAB} active={center.active === DEAL_TAB}>
          <div className="dock-scroll">{shownData !== null ? <DealLinks data={shownData} /> : <p className="dock-placeholder">{placeholder}</p>}</div>
        </CenterPanel>
        {searchLink !== null && <CenterPanel id={SEARCH_TAB} active={center.active === SEARCH_TAB}>
          {/* 開くまでは枠を作らない (案件を選ぶたびに Google を読みに行かない) */}
          {center.searchOpened && <LinkView key={searchLink.url} tab={searchLink} />}
        </CenterPanel>}
        {center.links.map(l => <CenterPanel key={l.id} id={l.id} active={center.active === l.id}>
          <LinkView tab={l} onClose={() => { center.close(l.id); }} />
        </CenterPanel>)}
      </div>
    </div>,
  };

  return <div className="crm-app cq-app">
    {writeBindings.conflict !== null && <ConflictDialog view={writeBindings.conflict} />}
    {/* 画面全体の読み上げ欄 (記録した・記録できない理由)。常に置いておき、中身だけ変える */}
    <p className="cq-sr-only" role="status" data-testid="screen-announcement">{announcement.text}{announcement.n % 2 === 1 ? '\u00a0' : ''}</p>
    <header className="crm-topbar cq-topbar"><a className="crm-home" href="/">HR_HR</a>
      <span className="crm-topbar-divider" /><h1 className="cq-title">架電</h1>
      <div className={`cq-mode cq-mode-${mode}`} role="status" aria-label="データの種類">
        <strong className="cq-mode-badge">{mode === 'live' ? '実データ(HubSpot)' : '架空サンプル'}</strong>
        <span className="cq-mode-note">{mode === 'live' ? (crmWrite.writesEnabled === true ? 'HubSpot への書き込みは、編集した項目だけです' : 'HubSpot への書き込みはしません') : '表示内容はすべて架空です。HubSpot には接続しません。'}</span>
        <span className="cq-mode-note-short">{mode === 'live' ? (crmWrite.writesEnabled === true ? '編集した項目だけ書き込み' : 'HubSpot 書き込みなし') : '架空・未接続'}</span>
      </div>
      {mode === 'live' && <button type="button" ref={zoomToggleRef} className={`cq-zoom-toggle is-${readiness}`} aria-expanded={drawerOpen}
        aria-controls="cq-zoom-drawer" data-testid="zoom-toggle" title={drawerOpen ? 'Zoom の枠を閉じる' : 'Zoom の枠を開く(消音・保留・通話を切る・サインインはこちら)'}
        onClick={() => { if (drawerOpen) closeZoom(); else openZoom(); }}>
        Zoom<span className="cq-zoom-dot" aria-hidden="true" /><span className="cq-zoom-state">{ZOOM_READINESS_LABELS[readiness]}</span></button>}
      <button type="button" className="cq-layout-reset" onClick={() => { dispatchLayout({ type: 'reset' }); }}
        title="パネルの置き場所と列の幅を、最初の配置に戻します(表示する項目の選択はそのまま)">元の配置に戻す</button>
      <span className="cq-mode-switch" role="group" aria-label="データの切り替え">
        <button type="button" aria-pressed={mode === 'live'} onClick={() => { switchMode('live'); }}>実データ</button>
        <button type="button" aria-pressed={mode === 'fixture'} disabled={inCall}
          title={inCall ? '通話中は切り替えられません(切り替えると電話の枠が閉じて通話が切れます)' : undefined}
          onClick={() => { switchMode('fixture'); }}>架空サンプル</button>
      </span>
    </header>

    <form className="cq-filters" aria-label="絞り込みと並び替え" onSubmit={e => { e.preventDefault(); update({ q: qDraft }); }}>
      <div className="cq-bar">
        <select className="cq-pipeline" aria-label="パイプライン" value={filters.pipeline}
          title={pipelines.labelsUnavailable ? 'HubSpot からパイプライン名を読めなかったため、設定上の呼び名で表示しています' : undefined}
          onChange={e => { changePipeline(e.target.value); }}>
          {pipelines.pipelines.map(p => <option key={p.id} value={p.id}>{pipelineName(p)}</option>)}</select>
        <input className="cq-search" type="search" aria-label="キーワード(会社名・案件名)" value={qDraft} maxLength={100}
          placeholder="会社名・案件名で検索" onChange={e => { setQDraft(e.target.value); }} />
        <OwnerFilter owner={filters.owner} effective={effectiveOwner} needsPick={needsOwnerPick}
          onChange={owner => { update({ owner }); }} owners={owners.state} onReload={owners.reload} />
        <select className="cq-sort" aria-label="並び替え" value={filters.sort} onChange={e => { update({ sort: e.target.value as QueueSort }); }}>
          {QUEUE_SORTS.map(s => <option key={s.value} value={s.value}>{s.label}</option>)}</select>
        <label className="cq-toggle"><input type="checkbox" checked={filters.due === 'today'}
          onChange={e => { update({ due: (e.target.checked ? 'today' : 'all') }); }} />次回日が来たものだけ</label>
        <button type="button" className="cq-btn cq-more-toggle" aria-expanded={panelOpen} aria-controls="cq-advanced"
          onClick={() => { setPanelOpen(o => !o); }}>詳細条件{detailCount > 0 && <span className="cq-badge">{detailCount}</span>}</button>
        <button type="button" className="cq-btn cq-btn-quiet" onClick={clearAll} disabled={!hasConditions && qDraft === ''}>条件をクリア</button>
      </div>
      {chips.length > 0 && <ul className="cq-chips" aria-label="適用中の条件">
        {chips.map(c => <li key={c.key} className="cq-chip"><span title={c.title}>{c.label}</span>
          <button type="button" aria-label={`「${c.label}」を外す`} onClick={() => { removeChip(c); }}>×</button></li>)}
      </ul>}
      <div id="cq-advanced" className="cq-advanced" hidden={!panelOpen}>
        {pipeline && <StageFilter key={`${mode}|${pipeline.id}`} pipeline={pipeline} selected={filters.stages}
          labelsUnavailable={pipelines.labelsUnavailable} onApply={stages => { update({ stages }); }} />}
        <div className="cq-ranges">
          <fieldset className="cq-range"><legend>次回架電日</legend>
            <label>から<input type="date" value={filters.nextFrom} onChange={e => { update({ nextFrom: e.target.value }); }} /></label>
            <label>まで<input type="date" value={filters.nextTo} onChange={e => { update({ nextTo: e.target.value }); }} /></label></fieldset>
          <fieldset className="cq-range"><legend>最終架電日</legend>
            <label>から<input type="date" value={filters.lastFrom} onChange={e => { update({ lastFrom: e.target.value }); }} /></label>
            <label>まで<input type="date" value={filters.lastTo} onChange={e => { update({ lastTo: e.target.value }); }} /></label></fieldset>
        </div>
      </div>
    </form>
    {layoutNote && <p className="cq-layout-note" role="status" data-testid="layout-note">{LAYOUT_UPDATED_NOTE}
      <button type="button" onClick={() => { setLayoutNote(false); }}>閉じる</button></p>}

    <LinkOpenerContext.Provider value={selectedId !== null ? openLinkInPanel : null}>
      <Dock layout={layout} dispatch={layoutDispatch} panels={panels} maximized={maxLinks ? 'links' : null} />
    </LinkOpenerContext.Provider>
    {/* Zoom の枠 (右から開く引き出し)。閉じている間も外さず、同じ大きさのまま画面の外へ送る
        (display:none にしない。iframe を作り直すと通話が切れ、発信の依頼も届かなくなる) */}
    <div id="cq-zoom-drawer" ref={drawerRef} className={`cq-zoom-drawer${drawerOpen ? ' is-open' : ''}`} data-testid="zoom-drawer"
      aria-hidden={!drawerOpen} inert={!drawerOpen}
      onKeyDown={e => { if (e.key === 'Escape' && drawerOpen) { e.preventDefault(); closeZoom(); } }}>
      <ZoomPhonePanel zoom={zoom} iframeRef={iframeRef} link={endedCall !== null ? 'selected' : callRecorded ? 'recorded' : 'none'} onClose={closeZoom} />
    </div>
  </div>;
}
