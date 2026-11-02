// URL の状態: ?view=team|today|board|detail&c=担当&flag=名札&band=区分&id=取引ID
export type View = 'team' | 'today' | 'board' | 'detail' | 'customer' | 'renewal' | 'results';

export interface Route {
  view: View;
  /** 担当 (コンサル担当者) で絞る */
  c: string;
  /** 注意の名札で絞る */
  flag: string;
  /** MTG が空いている期間の区分 (critical など) で絞る */
  band: string;
  /** 案件の詳細で開く取引 ID */
  id: string;
  /** 顧客と拠点で開く法人番号 */
  houjin: string;
}

export const VIEWS: { key: View; label: string; note: string; group: string }[] = [
  { key: 'team', label: 'チームと担当', note: '担当者ごとの持ち件数と注意の件数', group: '毎日' },
  { key: 'today', label: '今日', note: '今日動く案件', group: '毎日' },
  { key: 'board', label: '案件一覧', note: '稼働中の案件すべて', group: '毎日' },
  { key: 'detail', label: '案件の詳細', note: '接触履歴と契約の連なり', group: '毎日' },
  { key: 'customer', label: '顧客と拠点', note: '法人から拠点、契約へ。継続の経緯と成果', group: '顧客と契約' },
  { key: 'renewal', label: '満了と継続', note: '今月・来月・再来月に満了する契約', group: '顧客と契約' },
  { key: 'results', label: '成果と継続', note: '月に1回、継続・成果・記録の確認', group: '月1の確認' },
];

export function parseRoute(search: string): Route {
  const q = new URLSearchParams(search);
  const v = q.get('view');
  const view: View = v === 'today' || v === 'board' || v === 'detail' || v === 'customer' || v === 'renewal' || v === 'results' ? v : 'team';
  return { view, c: q.get('c') ?? '', flag: q.get('flag') ?? '', band: q.get('band') ?? '', id: q.get('id') ?? '', houjin: q.get('houjin') ?? '' };
}

export function routeHref(route: Route): string {
  const q = new URLSearchParams();
  if (route.view !== 'team') q.set('view', route.view);
  if (route.c !== '') q.set('c', route.c);
  if (route.flag !== '') q.set('flag', route.flag);
  if (route.band !== '') q.set('band', route.band);
  if (route.id !== '') q.set('id', route.id);
  if (route.houjin !== '') q.set('houjin', route.houjin);
  const s = q.toString();
  return s === '' ? window.location.pathname : `?${s}`;
}
