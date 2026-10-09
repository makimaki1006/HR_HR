/**
 * 「求人検索・リンク先」パネルで開くリンク (求人検索・ホームページ・求人媒体など) の判定と、タブの状態。
 *
 * - 開けるのは http(s) の URL だけ (javascript: / data: などは開かない)
 * - Google 検索は通常、枠の中への表示を断る。`igu=1` を付けると枠の中に出せる (2026-10-08 に Chrome で確認)。
 *   枠に出す URL は検索語 (q) と表示に関わる少数の項目だけを残して組み立て直す
 *   (HubSpot に貼られた URL には古いセッションの項目が多く付いているため)。
 *   「新しいタブで開く」は元の URL のまま開く
 * - HubSpot (app.hubspot.com / app-*.hubspot.com) は枠の拡張機能が入っているときだけ、通常のリンクと同じくパネルのタブ (枠) で開く。
 *   入っていないときは枠を断られるので、パネルのタブを作らず直接ブラウザの新しいタブで開く (`isHubspotUrl`)
 * - 枠の中への表示を断ると分かっているサイト (Yahoo・DuckDuckGo・自社ドメインなど) は枠を作らず「新しいタブで開く」だけを出す
 * - http: のページはこの画面 (https) の枠には出せない (混在コンテンツ) ので、同じく新しいタブだけ
 * - 断るかどうかは別オリジンなので確実には分からない。枠を出すときは必ず「新しいタブで開く」を並べる
 */

import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { formatPhoneForDisplay } from './phone';

/** 「リンク一覧」のタブの ID (常にある。以前の「案件」のタブ) */
export const DEAL_TAB = 'deal';
/** 求人検索のタブの ID (検索の URL があるときだけ) */
export const SEARCH_TAB = 'search';
/** 必要なときに開くリンクのタブの最大数 (リンク一覧・求人検索は数えない) */
export const MAX_LINK_TABS = 5;

/** http(s) の URL だけを返す (前後の空白は除く)。それ以外は null */
export function safeHttpUrl(raw: string | null | undefined): URL | null {
  if (typeof raw !== 'string') return null;
  const s = raw.trim();
  if (s === '' || s.length > 4000) return null;
  let u: URL;
  try { u = new URL(s); } catch { return null; }
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return null;
  if (u.hostname === '') return null;
  // 利用者名・パスワード入りの URL (https://user:pass@host) は見た目と行き先が食い違うので開かない
  if (u.username !== '' || u.password !== '') return null;
  return u;
}

const GOOGLE_HOSTS = new Set(['www.google.com', 'google.com', 'www.google.co.jp', 'google.co.jp']);
/** Google 検索の枠用 URL に残す項目 (検索語・言語・検索の種類・ページ) */
const GOOGLE_KEEP = ['q', 'hl', 'tbm', 'start'];

/** 枠の中への表示を断ると分かっているサイト (このドメインとそのサブドメイン) */
const NO_EMBED_DOMAINS = [
  'hubspot.com', 'hubspot.jp', 'zoom.us', 'accounts.google.com', 'mail.google.com', 'docs.google.com', 'drive.google.com',
  'yahoo.co.jp', 'yahoo.com', 'duckduckgo.com', 'f-a-c.co.jp',
];

function hostMatches(host: string, domain: string): boolean {
  return host === domain || host.endsWith(`.${domain}`);
}

/** HubSpot の画面 (app.hubspot.com / app-*.hubspot.com など) か。拡張機能が無いときは枠にもパネルのタブにも出さず、直接新しいタブで開く */
export function isHubspotUrl(u: URL): boolean {
  return hostMatches(u.hostname, 'hubspot.com') || hostMatches(u.hostname, 'hubspot.jp');
}

/** Google 検索の結果ページか */
export function isGoogleSearch(u: URL): boolean {
  return GOOGLE_HOSTS.has(u.hostname) && u.pathname === '/search';
}

/**
 * 枠 (iframe) に出す URL。出せない・出さないと分かっているときは null。
 * Google 検索は `igu=1` を付けて組み立て直す。Bing などはそのまま
 */
export function embedUrlFor(u: URL, hubspotEmbeddable = false): string | null {
  if (u.protocol !== 'https:') return null;
  if (isGoogleSearch(u)) {
    const out = new URL(`https://${u.hostname}/search`);
    for (const k of GOOGLE_KEEP) {
      const v = u.searchParams.get(k);
      if (v !== null) out.searchParams.set(k, v);
    }
    out.searchParams.set('igu', '1');
    return out.toString();
  }
  if (GOOGLE_HOSTS.has(u.hostname)) return null; // 検索以外の Google のページ (地図など) は枠を断る
  if (hubspotEmbeddable && isHubspotUrl(u)) return u.toString();
  if (NO_EMBED_DOMAINS.some(d => hostMatches(u.hostname, d))) return null;
  return u.toString();
}

/** 1 つの項目に入った URL を取り出す (改行・空白・読点・カンマ区切りで複数入ることがある)。http(s) だけ、重複なし */
export function extractUrls(raw: string | null | undefined): string[] {
  if (typeof raw !== 'string') return [];
  const out: string[] = [];
  for (const part of raw.split(/[\s,、;]+/u)) {
    const u = safeHttpUrl(part);
    if (u !== null && !out.includes(u.toString())) out.push(u.toString());
  }
  return out;
}

/** 求人検索の URL: 案件の「URL_求人検索」(bpo_32)。無ければ架ける番号で Google 検索 (番号 + 求人) を作る */
export function jobSearchUrl(storedUrl: string | null | undefined, phone: string | null | undefined): string | null {
  const stored = safeHttpUrl(storedUrl);
  if (stored !== null) return stored.toString();
  const p = typeof phone === 'string' ? phone.trim() : '';
  if (p === '') return null;
  const u = new URL('https://www.google.com/search');
  u.searchParams.set('q', `${p} 求人`);
  return u.toString();
}

/** 案件の求人検索の URL (「URL_求人検索」、無ければ架ける番号をハイフン区切りにして検索) */
export function dealJobSearchUrl(data: Pick<WorkspaceResponse, 'deal' | 'dial'>): string | null {
  const n = data.dial?.number ?? null;
  return jobSearchUrl(data.deal.job_search_url, n === null ? null : (formatPhoneForDisplay(n) ?? n));
}

/** 検索語が「求人」だけ・空の Google 検索 (番号が入っていない URL。開いても役に立たない) */
export function isEmptyGoogleSearch(raw: string): boolean {
  const u = safeHttpUrl(raw);
  if (u === null || !isGoogleSearch(u)) return false;
  return (u.searchParams.get('q') ?? '').replace(/求人/gu, '').trim() === '';
}

export interface LinkTab {
  id: string;
  /** タブに出す名前 (項目名、無ければホスト名) */
  label: string;
  /** 元の URL (「新しいタブで開く」はこれを開く) */
  url: string;
  host: string;
  /** 枠に出す URL。出せないときは null (「新しいタブで開く」だけを出す) */
  embed: string | null;
}

/** URL からタブを作る。http(s) でなければ null */
export function makeLinkTab(id: string, raw: string, label?: string, hubspotEmbeddable = false): LinkTab | null {
  const u = safeHttpUrl(raw);
  if (u === null) return null;
  const name = label?.trim() ?? '';
  return { id, label: name === '' ? u.hostname : name, url: u.toString(), host: u.hostname, embed: embedUrlFor(u, hubspotEmbeddable) };
}

export interface CenterTabsState {
  active: string;
  links: LinkTab[];
  /** 次に作るタブの番号 */
  seq: number;
}

export const initialCenterTabs: CenterTabsState = { active: DEAL_TAB, links: [], seq: 1 };

/**
 * リンクを開く。同じ URL のタブがあればそれを前に出す (求人検索のタブと同じ URL ならそちら)。
 * 上限を超えるときは、いちばん前に開いたタブを閉じてから開く
 */
export function openLink(s: CenterTabsState, raw: string, label: string | undefined, searchUrl: string | null, hubspotEmbeddable = false): CenterTabsState {
  const tab = makeLinkTab(`link-${String(s.seq)}`, raw, label, hubspotEmbeddable);
  if (tab === null) return s;
  if (searchUrl !== null && tab.url === searchUrl) return { ...s, active: SEARCH_TAB };
  const same = s.links.find(l => l.url === tab.url);
  if (same) return { ...s, active: same.id };
  const links = [...s.links, tab];
  while (links.length > MAX_LINK_TABS) links.shift();
  return { active: tab.id, links, seq: s.seq + 1 };
}

/** リンクのタブを閉じる。開いていたタブを閉じたら、隣のリンク (無ければ `fallback` = 求人検索か案件) を前に出す */
export function closeLink(s: CenterTabsState, id: string, fallback: string = DEAL_TAB): CenterTabsState {
  const i = s.links.findIndex(l => l.id === id);
  if (i < 0) return s;
  const links = s.links.filter(l => l.id !== id);
  if (s.active !== id) return { ...s, links };
  const next = links[i] ?? links[i - 1];
  return { ...s, links, active: next ? next.id : fallback };
}
