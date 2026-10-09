import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import type { KeyboardEvent, MouseEvent, ReactNode } from 'react';
import {
  closeLink, DEAL_TAB, initialCenterTabs, isHubspotUrl, makeLinkTab, openLink, safeHttpUrl, SEARCH_TAB,
} from './centerLinks';
import type { CenterTabsState, LinkTab } from './centerLinks';
import { FrameExtensionBadge } from './FrameExtensionBadge';

/**
 * 「求人検索・リンク先」パネルの中のタブ: 「リンク一覧」(いつもある) / 「求人検索」(検索の URL があるとき) / 開いたリンク (最大 5)。
 * タブは消さずに隠すだけなので、切り替えても枠の中のページは読み直さない。
 */

/** 枠に付ける sandbox (スクリプト・フォーム・別タブで開く は許す。上の画面の移動・ダウンロード等は許さない) */
export const LINK_FRAME_SANDBOX = 'allow-scripts allow-same-origin allow-forms allow-popups allow-popups-to-escape-sandbox';

type OpenLink = (url: string, label?: string) => void;
/** 項目のリンクを「求人検索・リンク先」パネルのタブで開く関数 (無ければ通常どおり新しいタブで開く) */
export const LinkOpenerContext = createContext<OpenLink | null>(null);

/**
 * 項目の値のリンク。クリックで「求人検索・リンク先」パネルのタブに開く。Ctrl / ⌘ / Shift / 中クリックはブラウザの新しいタブ (通常の動き)。
 * HubSpot の URL はパネルのタブを作らず、常にブラウザの新しいタブで直接開く。
 * http(s) でない値はリンクにせず文字のまま出す
 */
export function PropLink({ url, label, children }: { url: string; label?: string | undefined; children?: ReactNode }) {
  const open = useContext(LinkOpenerContext);
  const u = safeHttpUrl(url);
  if (u === null) return <span className="wd-link-text">{children ?? url}</span>;
  const href = u.toString();
  const hubspot = isHubspotUrl(u);
  function onClick(e: MouseEvent<HTMLAnchorElement>) {
    if (open === null || hubspot || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    open(href, label);
  }
  return <a className="wd-link" href={href} target="_blank" rel="noopener noreferrer" title={href} onClick={onClick}>{children ?? href}</a>;
}

export interface CenterTabs {
  active: string;
  links: LinkTab[];
  searchUrl: string | null;
  /** 求人検索のタブを一度でも開いたか (開くまでは枠を作らない = Google を読みに行かない) */
  searchOpened: boolean;
  open: OpenLink;
  close: (id: string) => void;
  activate: (id: string) => void;
}

/** パネルの中のタブの状態。`resetKey` (選んだ案件) が変わったら、開いたリンクを閉じてリンク一覧に戻す */
export function useCenterTabs(resetKey: string | null, searchUrl: string | null): CenterTabs {
  const [state, setState] = useState<CenterTabsState & { searchOpened: boolean }>({ ...initialCenterTabs, searchOpened: false });
  const [seenKey, setSeenKey] = useState(resetKey);
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setState({ ...initialCenterTabs, searchOpened: false });
  }
  const active = state.active === SEARCH_TAB && searchUrl === null ? DEAL_TAB : state.active;
  const open = useCallback<OpenLink>((url, label) => {
    setState(s => {
      const next = openLink(s, url, label, searchUrl);
      return { ...next, searchOpened: s.searchOpened || next.active === SEARCH_TAB };
    });
  }, [searchUrl]);
  // 最後のリンクを閉じたら、タブの並びで左隣 (求人検索があればそれ、無ければ案件) に戻る
  const close = useCallback((id: string) => {
    setState(s => ({ ...s, ...closeLink(s, id, searchUrl !== null ? SEARCH_TAB : DEAL_TAB) }));
  }, [searchUrl]);
  const activate = useCallback((id: string) => {
    setState(s => ({ ...s, active: id, searchOpened: s.searchOpened || id === SEARCH_TAB }));
  }, []);
  return { active, links: state.links, searchUrl, searchOpened: state.searchOpened, open, close, activate };
}

interface TabDef { id: string; label: string; title: string; closable: boolean }

export const tabDomId = (id: string) => `cq-ctab-${id}`;
export const panelDomId = (id: string) => `cq-cpanel-${id}`;

/** タブの並び (矢印キー・Home / End で移動して開く。Delete で開いたリンクのタブを閉じる) */
export function CenterTabBar({ tabs }: { tabs: CenterTabs }) {
  const defs: TabDef[] = [
    { id: DEAL_TAB, label: 'リンク一覧', title: '案件・会社に登録されたリンク', closable: false },
    ...(tabs.searchUrl !== null ? [{ id: SEARCH_TAB, label: '求人検索', title: 'Google で求人を検索した結果', closable: false }] : []),
    ...tabs.links.map(l => ({ id: l.id, label: l.label, title: l.url, closable: true })),
  ];
  const listRef = useRef<HTMLDivElement | null>(null);
  const focusTab = (id: string) => { listRef.current?.querySelector<HTMLElement>(`#${tabDomId(id)}`)?.focus(); };
  // リンクを開いた・通話が終わって案件に戻った等で、フォーカスしていた要素が隠れたら、前に出たタブへ移す
  // (隠れた中身にフォーカスが残ると、キーボードで迷子になる)
  useEffect(() => {
    const ae = document.activeElement;
    if (ae instanceof HTMLElement && ae.closest('.cq-cpanel[inert]') !== null) focusTab(tabs.active);
  });
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const i = defs.findIndex(d => d.id === tabs.active);
    let to: number | null = null;
    if (e.key === 'ArrowRight') to = (i + 1) % defs.length;
    else if (e.key === 'ArrowLeft') to = (i - 1 + defs.length) % defs.length;
    else if (e.key === 'Home') to = 0;
    else if (e.key === 'End') to = defs.length - 1;
    else if (e.key === 'Delete' && defs[i]?.closable) {
      e.preventDefault();
      tabs.close(tabs.active);
      // 閉じた後に前に出たタブへフォーカスを移す (描き直しの後)
      requestAnimationFrame(() => { listRef.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus(); });
      return;
    }
    const target = to === null ? undefined : defs[to];
    if (target === undefined) return;
    e.preventDefault();
    tabs.activate(target.id);
    focusTab(target.id);
  }
  return <div className="cq-ctabs" role="tablist" aria-label="求人検索・リンク先の表示" ref={listRef} onKeyDown={onKeyDown} data-testid="center-tabs">
    {defs.map(d => {
      const selected = d.id === tabs.active;
      return <span key={d.id} className={`cq-ctab${selected ? ' is-active' : ''}`} role="presentation">
        <button type="button" role="tab" id={tabDomId(d.id)} aria-selected={selected} aria-controls={panelDomId(d.id)}
          tabIndex={selected ? 0 : -1} title={d.title} onClick={() => { tabs.activate(d.id); }}>{d.label}</button>
        {/* 閉じるボタンはマウス用 (キーボードはタブで Delete、またはタブの中の「このタブを閉じる」) */}
        {d.closable && <button type="button" className="cq-ctab-close" tabIndex={-1} aria-label={`「${d.label}」のタブを閉じる`}
          title="このタブを閉じる" onClick={() => { tabs.close(d.id); }}>×</button>}
      </span>;
    })}
  </div>;
}

/** 1 つのタブの中身の枠。選ばれていない間も外さずに隠す (中身・スクロール位置を残す) */
export function CenterPanel({ id, active, tabbed = true, children }: {
  id: string; active: boolean;
  /** タブの並びを出していないとき (案件を選ぶ前) は false。tabpanel の役割を付けない */
  tabbed?: boolean; children: ReactNode;
}) {
  return <div role={tabbed ? 'tabpanel' : undefined} id={panelDomId(id)} aria-labelledby={tabbed ? tabDomId(id) : undefined}
    className={`cq-cpanel${active ? ' is-active' : ''}`} inert={!active} data-testid={`center-panel-${id}`}>{children}</div>;
}

const HINT_KEY = 'crm.linkHintDismissed';
const HINT_TEXT = '求人サイト（Indeed など）は枠の中では開けないことがあります。⌘ / Ctrl を押しながらクリックすると新しいタブで開きます。';
function readHintDismissed(): boolean {
  try { return window.localStorage.getItem(HINT_KEY) === '1'; } catch { return false; }
}

/** Google 検索のタブの先頭に出す 1 行の案内 (閉じた記録は localStorage に残す。使えなくても動く) */
function SearchHint() {
  const [hidden, setHidden] = useState(readHintDismissed);
  if (hidden) return null;
  return <div className="cq-linkhint" data-testid="link-hint">
    <span>{HINT_TEXT}</span>
    <button type="button" aria-label="案内を閉じる" onClick={() => {
      setHidden(true);
      try { window.localStorage.setItem(HINT_KEY, '1'); } catch { /* 保存できなくても閉じるだけ */ }
    }}>×</button>
  </div>;
}

/**
 * リンクのタブの中身: 細い操作行 (ホスト名・新しいタブで開く・戻る・再読み込み) + 枠。
 * 枠の中の移動は上の画面の履歴 (joint session history) に積まれるので、枠の中で 1 回以上移動したときだけ
 * 「戻る」で window.history.back() を呼ぶ (数が 0 のときは押せない = CRM 画面自体は戻らない)
 */
export function LinkView({ tab, onClose }: { tab: LinkTab; onClose?: (() => void) | undefined }) {
  const [reloads, setReloads] = useState(0);
  const [navs, setNavs] = useState(0);
  const loaded = useRef(false);
  /** 「戻る」で起きる枠の読み込み (移動ではない) を数えないための、まだ来ていない読み込みの数 */
  const pendingBack = useRef(0);
  const isSearch = tab.embed !== null && tab.host.replace(/^www\./, '').startsWith('google.');
  function onLoad() {
    if (!loaded.current) { loaded.current = true; return; }
    if (pendingBack.current > 0) { pendingBack.current -= 1; return; }
    setNavs(n => n + 1);
  }
  function back() {
    if (navs <= 0) return;
    pendingBack.current += 1;
    setNavs(n => n - 1);
    window.history.back();
  }
  function reload() {
    loaded.current = false;
    pendingBack.current = 0;
    setNavs(0);
    setReloads(n => n + 1);
  }
  return <div className="cq-linkview">
    <div className="cq-linkbar">
      <span className="cq-linkbar-host" title={tab.url}>{tab.host}</span>
      <FrameExtensionBadge />
      <a className="cq-linkbar-open" href={tab.url} target="_blank" rel="noopener noreferrer">新しいタブで開く</a>
      {tab.embed !== null && <button type="button" disabled={navs === 0} onClick={back}>戻る</button>}
      {tab.embed !== null && <button type="button" onClick={reload}>再読み込み</button>}
      {onClose && <button type="button" onClick={onClose}>このタブを閉じる</button>}
      {tab.embed !== null && <small className="cq-linkbar-note">表示されない場合は新しいタブで開いてください</small>}
    </div>
    {isSearch && <SearchHint />}
    {tab.embed !== null
      ? <iframe key={reloads} className="cq-linkframe" src={tab.embed} title={`${tab.label}(${tab.host})`}
        sandbox={LINK_FRAME_SANDBOX} referrerPolicy="no-referrer" data-testid="link-frame" onLoad={onLoad} />
      : <div className="cq-notice cq-empty cq-linkview-blocked">
        <strong>このページは画面の中に表示できません</strong>
        <p>{tab.host} のページは、ほかの画面の中に表示できない設定になっています。新しいタブで開いてください。</p>
        <a className="cq-linkbar-open" href={tab.url} target="_blank" rel="noopener noreferrer">新しいタブで開く</a>
      </div>}
  </div>;
}

/** 求人検索のタブの中身 (検索の URL をリンクのタブと同じ形にする) */
export function searchTab(url: string): LinkTab | null {
  return makeLinkTab(SEARCH_TAB, url, '求人検索');
}
