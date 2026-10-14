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

import type { CallQueueItem } from '../../generated/CallQueueItem';
import type { WorkspaceResponse } from '../../generated/WorkspaceResponse';
import { formatPhoneForDisplay } from './phone';

/** 求人検索のタブの ID (いつもある。案件を選んだときの最初のタブ) */
export const SEARCH_TAB = 'search';
/** ホームページ・求人票のタブの ID (いつもある。URL が無いときは中身に案内だけ出す) */
export const HOME_TAB = 'home';
export const POSTING_TAB = 'posting';
/** 会社のサイト・求人媒体のタブの ID (あるときだけ。求人媒体は `media-0` から) */
export const SITE_TAB = 'site';
/** 必要なときに開くリンクのタブの最大数 (固定のタブは数えない) */
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

/** 求人検索の切り替え: 架ける番号で探す / 会社名 (事業所名) で探す */
export type SearchMode = 'phone' | 'company';
export const SEARCH_MODES: readonly { id: SearchMode; label: string }[] = [
  { id: 'phone', label: '電話番号' },
  { id: 'company', label: '会社名' },
];

function googleJobSearch(word: string): string | null {
  const w = word.trim();
  if (w === '') return null;
  const u = new URL('https://www.google.com/search');
  u.searchParams.set('q', `${w} 求人`);
  return u.toString();
}

/**
 * 求人検索の URL。
 * 電話番号: 架ける番号 (ハイフン区切り) + 求人。番号が無いときだけ、案件の「URL_求人検索」(bpo_32。同じ番号の検索) を使う。
 * 会社名: 会社名 + 求人
 */
export function searchUrlFor(mode: SearchMode, input: { phone: string | null | undefined; company: string | null | undefined; stored?: string | null | undefined }): string | null {
  if (mode === 'company') return googleJobSearch(input.company ?? '');
  return googleJobSearch(input.phone ?? '') ?? safeHttpUrl(input.stored)?.toString() ?? null;
}

/** 求人検索の番号 (ハイフン区切りにして検索) */
const displayPhone = (n: string | null | undefined): string | null => (typeof n === 'string' && n.trim() !== '' ? (formatPhoneForDisplay(n) ?? n) : null);

/** 会社名 (事業所名): 取引の主担当会社の名前、無ければ案件名。事業所名を表す専用のプロパティは CRM に無い */
export function dealCompanyName(data: Pick<WorkspaceResponse, 'deal' | 'companies'>): string | null {
  const c = data.companies.find(x => x.is_primary) ?? null;
  const name = (c?.name ?? '').trim() || (data.deal.name ?? '').trim();
  return name === '' ? null : name;
}

/** 案件の求人検索の URL */
export function dealJobSearchUrl(data: Pick<WorkspaceResponse, 'deal' | 'dial'> & Partial<Pick<WorkspaceResponse, 'companies'>>, mode: SearchMode = 'phone'): string | null {
  return searchUrlFor(mode, {
    phone: displayPhone(data.dial?.number), stored: data.deal.job_search_url,
    company: dealCompanyName({ deal: data.deal, companies: data.companies ?? [] }),
  });
}

/** 一覧の行 (詳細を読み込む前) の求人検索の URL。詳細が届いたあとも同じ URL になるよう、同じ組み立てを使う */
export function rowJobSearchUrl(item: Pick<CallQueueItem, 'phone' | 'company' | 'deal_name'>, mode: SearchMode = 'phone'): string | null {
  return searchUrlFor(mode, { phone: displayPhone(item.phone), company: (item.company?.name ?? '').trim() || item.deal_name });
}

/** 詳細を読み込む前の固定のタブ。求人検索だけは一覧の行の情報で先に開き、ほかは案内だけ */
export function waitingFixedLinks(item: Pick<CallQueueItem, 'phone' | 'company' | 'deal_name'> | null, mode: SearchMode, waiting: string): FixedLink[] {
  return [
    { id: SEARCH_TAB, label: '求人検索', url: item === null ? null : rowJobSearchUrl(item, mode), missing: item === null ? waiting : SEARCH_MISSING[mode] },
    { id: HOME_TAB, label: 'ホームページ', url: null, missing: waiting },
    { id: POSTING_TAB, label: '求人票', url: null, missing: waiting },
  ];
}

const SEARCH_MISSING: Record<SearchMode, string> = {
  phone: 'この案件には架ける番号がありません。「会社名」に切り替えて探せます。',
  company: 'この案件には会社名がありません。「電話番号」に切り替えて探せます。',
};

/** どちらも Google 検索で、検索語 (q) が同じか */
function sameGoogleQuery(a: string, b: string): boolean {
  const ua = safeHttpUrl(a); const ub = safeHttpUrl(b);
  if (ua === null || ub === null || !isGoogleSearch(ua) || !isGoogleSearch(ub)) return false;
  const qa = (ua.searchParams.get('q') ?? '').trim();
  return qa !== '' && qa === (ub.searchParams.get('q') ?? '').trim();
}

/** 検索語が「求人」だけ・空の Google 検索 (番号が入っていない URL。開いても役に立たない) */
export function isEmptyGoogleSearch(raw: string): boolean {
  const u = safeHttpUrl(raw);
  if (u === null || !isGoogleSearch(u)) return false;
  return (u.searchParams.get('q') ?? '').replace(/求人/gu, '').trim() === '';
}

/** 固定のタブ (閉じられない)。url が null のときもタブは出し、中身に `missing` の案内を出す */
export interface FixedLink {
  id: string;
  label: string;
  url: string | null;
  /** url が無いときに中身へ出す案内 */
  missing: string;
}

/**
 * 案件のリンクから固定のタブを作る。並びは 求人検索 / ホームページ / 求人票 を必ず、
 * そのあとに 会社のサイト (ホームページと違うときだけ) と 求人媒体 (あるだけ)。
 * ホームページは案件の「ホームページ」、無ければ主担当会社のサイト
 */
export function dealFixedLinks(data: WorkspaceResponse, mode: SearchMode = 'phone'): FixedLink[] {
  const d = data.deal;
  const company = data.companies.find(c => c.is_primary) ?? null;
  const dealHome = safeHttpUrl(d.homepage_url)?.toString() ?? null;
  const site = safeHttpUrl(company?.website)?.toString() ?? null;
  const home = dealHome ?? site;
  const posting = d.job_posting_url !== null && !isEmptyGoogleSearch(d.job_posting_url) ? safeHttpUrl(d.job_posting_url)?.toString() ?? null : null;
  const media = extractUrls(d.media_job_urls);
  const out: FixedLink[] = [
    { id: SEARCH_TAB, label: '求人検索', url: dealJobSearchUrl(data, mode), missing: SEARCH_MISSING[mode] },
    { id: HOME_TAB, label: 'ホームページ', url: home, missing: 'この案件にはホームページが登録されていません。' },
    { id: POSTING_TAB, label: '求人票', url: posting, missing: 'この案件には求人票が登録されていません。' },
  ];
  if (site !== null && site !== home) out.push({ id: SITE_TAB, label: '会社のサイト', url: site, missing: '' });
  media.forEach((u, i) => {
    out.push({ id: `media-${String(i)}`, label: media.length > 1 ? `求人媒体 ${String(i + 1)}` : '求人媒体', url: u, missing: '' });
  });
  return out;
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

export const initialCenterTabs: CenterTabsState = { active: SEARCH_TAB, links: [], seq: 1 };

/**
 * リンクを開く。同じ URL のタブがあればそれを前に出す (固定のタブと同じ URL ならそちら)。
 * 上限を超えるときは、いちばん前に開いたタブを閉じてから開く
 */
export function openLink(s: CenterTabsState, raw: string, label: string | undefined, fixed: readonly Pick<FixedLink, 'id' | 'url'>[], hubspotEmbeddable = false): CenterTabsState {
  const tab = makeLinkTab(`link-${String(s.seq)}`, raw, label, hubspotEmbeddable);
  if (tab === null) return s;
  // 同じ URL、または同じ検索語の Google 検索 (HubSpot に貼られた古い URL など) は、求人検索のタブを前に出す
  const fixedSame = fixed.find(f => f.url !== null && (f.url === tab.url || (f.id === SEARCH_TAB && sameGoogleQuery(f.url, tab.url))));
  if (fixedSame) return { ...s, active: fixedSame.id };
  const same = s.links.find(l => l.url === tab.url);
  if (same) return { ...s, active: same.id };
  const links = [...s.links, tab];
  while (links.length > MAX_LINK_TABS) links.shift();
  return { active: tab.id, links, seq: s.seq + 1 };
}

/** リンクのタブを閉じる。開いていたタブを閉じたら、隣のリンク (無ければ `fallback` = 求人検索) を前に出す */
export function closeLink(s: CenterTabsState, id: string, fallback: string = SEARCH_TAB): CenterTabsState {
  const i = s.links.findIndex(l => l.id === id);
  if (i < 0) return s;
  const links = s.links.filter(l => l.id !== id);
  if (s.active !== id) return { ...s, links };
  const next = links[i] ?? links[i - 1];
  return { ...s, links, active: next ? next.id : fallback };
}
